//! Description search on real photographs.
//!
//! Every other image-arm number in this repo is from screenshots, and the
//! screenshot result is unambiguous: reading the text out of one and matching it
//! retrieves 0.964, while the image arm manages 0.268. That is a fact about
//! screenshots, which are text documents with a GUI.
//!
//! It is not evidence about photographs, and the difference matters for what the
//! product can honestly claim. A photograph of a dog has no text to read, so the
//! lexical arm has nothing to match on and the image arm is the *only* arm. This
//! measures that case.
//!
//! The corpus is stock imagery with a known label per file, so the query is a
//! description rather than the image's own OCR text. Set `TESSERA_PHOTOS` to a
//! folder of `label.jpg` files where `label` describes the picture.
//!
//! ```
//! cargo test --release -p tessera-core --test image_photos -- --ignored --nocapture
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

#[test]
#[ignore = "needs the siglip model set and a labelled photo corpus"]
fn description_search_on_real_photographs() {
    let models = models();
    let folder = match std::env::var("TESSERA_PHOTOS") {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            eprintln!(
                "\n== photographs ==\nNo corpus. Set TESSERA_PHOTOS to a folder of images whose\n\
                 file stems describe them, e.g. `a dog running through grass.jpg`. Every\n\
                 figure in this file comes from screenshots, which are the one corpus\n\
                 the image arm is not for; this is the number that says whether the\n\
                 README can claim 'search by description'."
            );
            return;
        }
    };
    assert!(folder.is_dir(), "no folder at {}", folder.display());

    let mut labelled: Vec<(PathBuf, String)> = std::fs::read_dir(&folder)
        .expect("readable")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("jpg" | "jpeg" | "png")
            )
        })
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?.replace(['-', '_'], " ");
            (stem.len() > 4).then_some((path, stem))
        })
        .collect();
    labelled.sort();
    assert!(
        labelled.len() >= 8,
        "{} labelled images is too few to say anything",
        labelled.len()
    );

    for precision in [Precision::Int8, Precision::Fp32] {
        let Ok(embedder) = SiglipEmbedder::with_precision(&models, 4, precision) else {
            eprintln!("\n== {precision:?} == not installed, skipped");
            continue;
        };

        let mut images: Vec<Embedding> = Vec::new();
        let mut labels: Vec<String> = Vec::new();
        for (path, label) in &labelled {
            let Ok(embedding) = embedder.embed_image(path, DIM) else {
                continue;
            };
            images.push(embedding);
            labels.push(label.clone());
        }

        let mut floors = Vec::new();
        for i in 0..images.len() {
            for j in (i + 1)..images.len() {
                floors.push(cosine(&images[i], &images[j]));
            }
        }
        floors.sort_by(|a, b| a.total_cmp(b));
        let median = floors[floors.len() / 2];

        let (mut top1, mut in5) = (0usize, 0usize);
        for (i, label) in labels.iter().enumerate() {
            let Ok(query) = embedder.embed_text(label, DIM) else {
                continue;
            };
            let mut scored: Vec<(usize, f32)> = images
                .iter()
                .enumerate()
                .map(|(j, e)| (j, cosine(&query, e)))
                .collect();
            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
            let rank = scored
                .iter()
                .position(|(j, _)| *j == i)
                .unwrap_or(usize::MAX);
            top1 += usize::from(rank == 0);
            in5 += usize::from(rank < 5);
            if rank != 0 {
                eprintln!(
                    "  miss {label:?}: expected {i}, got {:?} at {:.3} (target at {:.3})",
                    scored[0].0,
                    scored[0].1,
                    scored
                        .iter()
                        .position(|(j, _)| *j == i)
                        .map(|r| scored[r].1)
                        .unwrap_or(f32::MIN)
                );
            }
        }
        let n = labels.len().max(1) as f64;
        let chance = 1.0 / images.len().max(1) as f64;

        eprintln!(
            "\n== photographs ({precision:?}) ==\ncorpus {n} labelled photographs\n\
             noise floor: median unrelated cosine {median:.3}\n\
             top-1 {:.3} | recall@5 {:.3} | chance {chance:.3}",
            top1 as f64 / n,
            in5 as f64 / n
        );
    }
}
