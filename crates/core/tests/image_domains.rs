//! Does fp32 SigLIP beat the int8 export we actually ship?
//!
//! Ticket 07's diagnosis was that the int8 export flattens the embedding space:
//! two unrelated screenshots sat at a median cosine of 0.508, which matched the
//! week-1 spike's independently measured noise floor of 0.55. The fp32 weights
//! (776 MB, pinned in `lock.json`) are the obvious way to find out whether that
//! flatness was the quantisation's doing or a property of the corpus.
//!
//! This runs both precisions over the same corpus, same machine, same session,
//! and reports the noise floor and top-1 for each.
//!
//! ```
//! cargo test --release -p tessera-core --test image_precision -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use tessera_core::models::Models;
use tessera_core::vectors::siglip::{Precision, SiglipEmbedder};
use tessera_core::vectors::{cosine, Embedding};

const DIM: usize = 768;

fn models() -> Models {
    let dir = std::env::var("TESSERA_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            tessera_core::cli::models_dir(
                &tessera_core::cli::default_data_dir().expect("a data dir"),
                None,
            )
        });
    Models::open(&dir).expect("models dir")
}

fn screenshots() -> Option<PathBuf> {
    let folder = std::env::var("TESSERA_SCREENSHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            PathBuf::from(home)
                .join("OneDrive")
                .join("Pictures")
                .join("Screenshots")
        });
    folder.is_dir().then_some(folder)
}

#[test]
#[ignore = "needs the siglip model set"]
fn fp32_and_int8_are_the_same_space_with_different_resolution() {
    let models = models();
    let folder = screenshots().expect("a screenshot folder");
    let ocr = tessera_core::ocr::select(&tessera_core::ocr::OcrChoice::Native, &models).unwrap();

    let mut all: Vec<PathBuf> = std::fs::read_dir(&folder)
        .expect("readable")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("png" | "jpg" | "jpeg")
            )
        })
        .collect();
    all.sort();
    let limit = std::env::var("TESSERA_CORPUS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(50usize);
    if all.len() > limit {
        let stride = all.len() / limit;
        all = (0..limit).map(|i| all[i * stride].clone()).collect();
    }

    for precision in [Precision::Int8, Precision::Fp32] {
        let Ok(embedder) = SiglipEmbedder::with_precision(&models, 4, precision) else {
            eprintln!("\n== {precision:?} == not installed, skipped");
            continue;
        };

        let mut images: Vec<Embedding> = Vec::new();
        let mut texts: Vec<String> = Vec::new();
        for path in &all {
            let Ok(embedding) = embedder.embed_image(path, DIM) else {
                continue;
            };
            let text = ocr.engine.recognize(path).unwrap_or_default();
            if text.trim().is_empty() {
                continue;
            }
            images.push(embedding);
            texts.push(text.split_whitespace().collect::<Vec<_>>().join(" "));
        }

        let mut floors = Vec::new();
        for i in 0..images.len() {
            for j in (i + 1)..images.len() {
                floors.push(cosine(&images[i], &images[j]));
            }
        }
        floors.sort_by(|a, b| a.total_cmp(b));
        let median = floors[floors.len() / 2];
        let p5 = floors[floors.len() / 20];

        let mut ranked = 0usize;
        for (i, text) in texts.iter().enumerate() {
            let Ok(query) = embedder.embed_text(text, DIM) else {
                continue;
            };
            let best_other = images
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, e)| cosine(&query, e))
                .fold(f32::MIN, f32::max);
            if cosine(&query, &images[i]) > best_other {
                ranked += 1;
            }
        }
        let n = texts.len().max(1) as f64;

        eprintln!(
            "\n== {precision:?} ==\ncorpus {n} images\nnoise floor: p5 {p5:.3}, median {median:.3}\n\
             top-1 {:.3}",
            ranked as f64 / n
        );
    }

    eprintln!(
        "\nIf the noise floor did not fall and top-1 did not rise, fp32 is not the\n\
         lever the flatness diagnosis said it was - which is worth knowing, because\n\
         it would mean the number is bounded by the corpus rather than by the\n\
         quantisation."
    );
}
