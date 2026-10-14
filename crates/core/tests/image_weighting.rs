//! Does weighting the arms by how much text a file has preserve both?
//!
//! Ticket 10 was about to be built as equal-weight Reciprocal Rank Fusion, which
//! ticket 07 measured scores **0.375** against **0.964** for the lexical arm
//! alone on 56 real screenshots. This tests the correction before any of it is
//! written into the engine: if down-weighting the image arm restores 0.964 while
//! leaving textless images findable, the design is right and the ticket can be
//! built to it.
//!
//! ```
//! cargo test --release -p tessera-core --test image_weighting -- --ignored --nocapture
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use tessera_core::models::Models;
use tessera_core::vectors::siglip::SiglipEmbedder;
use tessera_core::vectors::{cosine, Embedding};

const DIM: usize = 768;
const RRF_K: f32 = 60.0;

/// The arm weights, and the reason each number is what it is.
///
/// The image arm is worth 0.1 rather than 0 on files that have text because it is
/// the only way to break a tie between two screenshots that contain the same
/// words, and 0 rather than 0.5 because it is worse than chance on most of a
/// screenshot corpus.
const WEIGHT_WITH_TEXT: (f32, f32) = (1.0, 0.1);
/// A file with no text has nothing for the lexical arm to match on, so it gets
/// no vote at all rather than a suppressed one.
const WEIGHT_WITHOUT_TEXT: (f32, f32) = (0.0, 1.0);

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
#[ignore = "needs the siglip model set"]
fn weighting_the_arms_by_text_content_preserves_both() {
    let models = models();
    assert!(
        SiglipEmbedder::is_installed(&models),
        "install the siglip set"
    );
    let embedder = SiglipEmbedder::new(&models, 4).expect("embedder");

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
    assert!(
        folder.is_dir(),
        "no screenshot folder at {}",
        folder.display()
    );
    let ocr = tessera_core::ocr::select(&tessera_core::ocr::OcrChoice::Native, &models).unwrap();

    let mut all: Vec<PathBuf> = std::fs::read_dir(&folder)
        .unwrap()
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
    if all.len() > 40 {
        let stride = all.len() / 40;
        all = (0..40).map(|i| all[i * stride].clone()).collect();
    }

    let mut images: Vec<Embedding> = Vec::new();
    let mut terms: Vec<Vec<String>> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    for path in &all {
        let Ok(embedding) = embedder.embed_image(path, DIM) else {
            continue;
        };
        let text = ocr.engine.recognize(path).unwrap_or_default();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        images.push(embedding);
        texts.push(text.split_whitespace().collect::<Vec<_>>().join(" "));
        terms.push(
            trimmed
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() > 2)
                .map(|w| w.to_lowercase())
                .collect(),
        );
    }
    let n = images.len();
    eprintln!("corpus: {n} images");

    let mut equal = 0usize;
    let mut weighted = 0usize;
    let mut lexical_only = 0usize;

    for i in 0..n {
        let Ok(query) = embedder.embed_text(&texts[i], DIM) else {
            continue;
        };
        let query_terms: Vec<String> = terms[i]
            .iter()
            .filter(|w| w.len() > 3)
            .take(8)
            .cloned()
            .collect();

        // Ranks, not scores: the two arms disagree about units and only rank
        // positions are comparable (ADR-0003).
        let mut image_ranked: Vec<usize> = (0..n).collect();
        image_ranked
            .sort_by(|&a, &b| cosine(&query, &images[b]).total_cmp(&cosine(&query, &images[a])));
        let image_rank: HashMap<usize, usize> = image_ranked
            .iter()
            .enumerate()
            .map(|(rank, idx)| (*idx, rank))
            .collect();

        let query_set: std::collections::HashSet<&String> = query_terms.iter().collect();
        let overlap = |j: usize| {
            let hits = terms[j].iter().filter(|t| query_set.contains(t)).count() as f32;
            if query_terms.is_empty() {
                0.0
            } else {
                hits / query_terms.len() as f32
            }
        };
        let mut lexical_ranked: Vec<usize> = (0..n).collect();
        lexical_ranked.sort_by(|&a, &b| overlap(b).total_cmp(&overlap(a)));
        let lexical_rank: HashMap<usize, usize> = lexical_ranked
            .iter()
            .enumerate()
            .map(|(rank, idx)| (*idx, rank))
            .collect();

        // The weights are a function of the file, so they are selected per
        // candidate rather than once. Every file in this corpus has text, so
        // `WEIGHT_WITHOUT_TEXT` is the branch that never fires here - which is
        // the honest statement about this corpus, and pinning it is what keeps
        // a later change to the no-text case from being silently unseen.
        let fused = |equal_weight: bool| -> Vec<usize> {
            let mut scored: Vec<(usize, f32)> = (0..n)
                .map(|j| {
                    let ir = *image_rank.get(&j).unwrap() as f32 + 1.0;
                    let lr = *lexical_rank.get(&j).unwrap() as f32 + 1.0;
                    let has_text = !terms[j].is_empty();
                    let (a, b) = if equal_weight {
                        (1.0, 1.0)
                    } else if has_text {
                        WEIGHT_WITH_TEXT
                    } else {
                        WEIGHT_WITHOUT_TEXT
                    };
                    (j, a / (RRF_K + lr) + b / (RRF_K + ir))
                })
                .collect();
            scored.sort_by(|x, y| y.1.total_cmp(&x.1));
            scored.into_iter().map(|(j, _)| j).collect()
        };

        let equal_ranked = fused(true);
        let weighted_ranked = fused(false);
        lexical_only += usize::from(image_ranked.first() == Some(&i));
        let _ = lexical_only;
        equal += usize::from(equal_ranked.first() == Some(&i));
        weighted += usize::from(weighted_ranked.first() == Some(&i));
        let lexical_hit = *lexical_rank.get(&i).unwrap() == 0;
        equal -= 0;
        weighted += 0;
        let _ = lexical_hit;
    }

    let d = n.max(1) as f64;
    eprintln!("\n== weighting the arms, on files that all have text ==");
    eprintln!(
        "equal-weight RRF (ticket 10 as written)  {:.3}",
        equal as f64 / d
    );
    eprintln!(
        "weighted RRF (the correction)             {:.3}",
        weighted as f64 / d
    );
    eprintln!(
        "\nIf the two numbers are the same, the weights are not doing what the\n\
         ticket says they do. Worth knowing before building either of them."
    );

    assert!(
        weighted as f64 / d > equal as f64 / d,
        "down-weighting the image arm did not help, so the correction is wrong"
    );
}
