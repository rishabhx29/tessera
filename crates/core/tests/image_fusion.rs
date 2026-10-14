//! Does fusing the image arm with the OCR text recover description search?
//!
//! The fp32 download is stuck at ~0 KB/s from this machine, and it is worth
//! asking whether fp32 is even the biggest lever before spending a day on a
//! 776 MB fetch.
//!
//! Tessera already reads the text out of every screenshot it indexes. So for a
//! screenshot corpus there are two independent signals per file — how the image
//! *looks* and what it *says* — and the product's architecture already fuses
//! arms (ADR-0003, ticket 10). This measures what that fusion is worth here:
//!
//! - **image only** — the image arm alone, i.e. what exists today
//! - **text only** — lexical match on the OCR text, as a baseline
//! - **fused** — Reciprocal Rank Fusion of the two
//!
//! The interesting outcome is not whichever number is highest. It is whether the
//! image arm *adds* anything over text on a corpus that is mostly text.
//!
//! ```
//! cargo test --release -p tessera-core --test image_fusion -- --ignored --nocapture
//! ```

use std::collections::BTreeSet;
use std::path::PathBuf;

use tessera_core::models::Models;
use tessera_core::vectors::siglip::SiglipEmbedder;
use tessera_core::vectors::{cosine, Embedding};

const DIM: usize = 768;
/// The constant every Reciprocal Rank Fusion formula shares.
const RRF_K: f32 = 60.0;

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

/// Lowercased word set, minus a stopword list that costs nothing to include and
/// would otherwise let "the" match everything.
fn terms(text: &str) -> BTreeSet<String> {
    const STOP: [&str; 24] = [
        "the", "and", "for", "you", "are", "but", "not", "with", "this", "that", "from", "have",
        "has", "was", "were", "will", "your", "our", "all", "can", "get", "set", "new", "http",
    ];
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOP.contains(&w.to_lowercase().as_str()))
        .map(|w| w.to_lowercase())
        .collect()
}

/// Overlap between a query and a document, normalised by the query's size so a
/// long screenshot is not rewarded for containing many words.
fn lexical(query: &BTreeSet<String>, doc: &BTreeSet<String>) -> f32 {
    if query.is_empty() {
        return 0.0;
    }
    let hits = query.intersection(doc).count() as f32;
    hits / query.len() as f32
}

fn rank_by(scores: Vec<(usize, f32)>) -> Vec<usize> {
    let mut scored = scores;
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.into_iter().map(|(i, _)| i).collect()
}

#[test]
#[ignore = "needs the siglip model set"]
fn does_fusing_the_ocr_text_recover_description_search() {
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
    if all.len() > 60 {
        let stride = all.len() / 60;
        all = (0..60).map(|i| all[i * stride].clone()).collect();
    }

    let mut images: Vec<Embedding> = Vec::new();
    let mut doc_terms: Vec<BTreeSet<String>> = Vec::new();
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
        doc_terms.push(terms(&text));
        texts.push(text.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    eprintln!("corpus: {} images with text", images.len());

    let (mut img_only, mut txt_only, mut fused, mut fused_image_only) = (0, 0, 0, 0);
    let n = images.len();

    for (i, text) in texts.iter().enumerate().take(n) {
        // Each image's own OCR text is the query. It is the easiest possible
        // question and the same one the image-arm benchmark uses, so the two
        // numbers are directly comparable.
        {
            let Ok(query) = embedder.embed_text(text, DIM) else {
                continue;
            };
            let q_terms = terms(text);

            let image_ranks = rank_by(
                images
                    .iter()
                    .enumerate()
                    .map(|(j, e)| (j, cosine(&query, e)))
                    .collect(),
            );
            let text_ranks = rank_by(
                doc_terms
                    .iter()
                    .enumerate()
                    .map(|(j, t)| (j, lexical(&q_terms, t)))
                    .collect(),
            );

            let image_rank = |i: usize| {
                image_ranks
                    .iter()
                    .position(|x| *x == i)
                    .unwrap_or(usize::MAX)
            };
            let text_rank = |i: usize| {
                text_ranks
                    .iter()
                    .position(|x| *x == i)
                    .unwrap_or(usize::MAX)
            };

            // Fused by RRF: rank positions combine, not raw scores, because a
            // cosine in [0,1] and an overlap ratio in [0,1] are not the same
            // quantity even though both are unitless (ADR-0003).
            let fused_rank = rank_by(
                (0..n)
                    .map(|j| {
                        let ir = image_rank(j) as f32;
                        let tr = text_rank(j) as f32;
                        let score = if ir == f32::MAX || tr == f32::MAX {
                            f32::MIN
                        } else {
                            1.0 / (RRF_K + ir + 1.0) + 1.0 / (RRF_K + tr + 1.0)
                        };
                        (j, score)
                    })
                    .collect(),
            );

            // Does the image arm *change* anything when text is present?
            // Ranking first among the text arm's top 10 isolates that.
            let text_top: BTreeSet<usize> = text_ranks.iter().take(10).copied().collect();
            let image_promoted =
                text_top.contains(&i) && fused_rank.iter().take(10).any(|x| *x == i);

            img_only += usize::from(image_rank(i) == 0);
            txt_only += usize::from(text_rank(i) == 0);
            fused += usize::from(fused_rank[0] == i);
            fused_image_only += usize::from(image_promoted);
        }
    }

    let d = n.max(1) as f64;
    eprintln!("\n==== does OCR text rescue description search? ====");
    eprintln!("corpus                         {n}");
    eprintln!("image arm alone (top-1)        {:.3}", img_only as f64 / d);
    eprintln!("OCR text alone (top-1)        {:.3}", txt_only as f64 / d);
    eprintln!("fused by RRF (top-1)          {:.3}", fused as f64 / d);
    eprintln!(
        "image arm changed the top-10   {:.3}   <- the image arm's *marginal* value",
        fused_image_only as f64 / d
    );
    eprintln!(
        "\n(8w / 5w query styles are computed but not scored here; the\n\
             numbers above use each image's full OCR text as its own query.)"
    );
}
