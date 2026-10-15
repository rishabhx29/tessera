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
//! Every file here has text, so this is the corpus where the image arm is
//! weakest and the one where down-weighting it has to be justified rather than
//! asserted. The sweep at the foot of the test reports the top-1, top-10 and
//! kept-in-top-10 accuracy for each value of the image arm's vote, from 0
//! (lexical alone) to 1 (equal weight), and how many queries fusion rescued that
//! text alone missed. It found the cliff between 0.05 and 0.06 — so 0.1, which an
//! earlier run of this corpus endorsed, costs 10.7 points — and it found that
//! `rescued` is 0 at every weight, on a denominator of 2. The lexical arm misses
//! 2 of these 56 queries, so this corpus cannot show what a second arm adds; it
//! can only show what one costs. The sweep reports that, and the ticket picks the
//! constant from it, without either claiming a benefit the corpus cannot see.
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

/// Reciprocal Rank Fusion with the image arm's vote scaled.
///
/// Ranks are positions, so this is the arithmetic ADR-0003 specifies — never a
/// raw score, because an overlap ratio and a cosine are not the same quantity.
/// `w_image` scales one arm's contribution; `1.0` is the equal-weight fusion
/// ADR-0003 describes and `0.0` is the lexical arm alone.
fn fuse(image_rank: &[usize], text_rank: &[usize], n: usize, w_image: f32) -> Vec<usize> {
    // Both arms rank every file, so a position always exists; an absent one
    // would score as the worst rank rather than being dropped, which is how RRF
    // treats a document one arm did not return.
    let position = |ranks: &[usize], i: usize| {
        ranks
            .iter()
            .position(|x| *x == i)
            .map_or(n as f32, |position| position as f32)
    };
    rank_by(
        (0..n)
            .map(|j| {
                let image_rank = position(image_rank, j);
                let text_rank = position(text_rank, j);
                (
                    j,
                    1.0 / (RRF_K + text_rank + 1.0) + w_image / (RRF_K + image_rank + 1.0),
                )
            })
            .collect(),
    )
}

/// The image arm's vote, swept. `1.0` reproduces equal-weight RRF and `0.0` is
/// the lexical arm on its own, so the two constants ticket 10 argues about are
/// both here as endpoints of the same curve. The steps are fine between 0.04 and
/// 0.10 because the first sweep put a cliff somewhere in that interval: 0.05 held
/// 0.964 and 0.10 had already fallen to 0.857.
const IMAGE_WEIGHTS: [f32; 14] = [
    0.0, 0.02, 0.04, 0.05, 0.06, 0.07, 0.08, 0.09, 0.1, 0.15, 0.2, 0.35, 0.5, 1.0,
];

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

    let n = images.len();
    let (mut img_only, mut txt_only, mut scored, mut lexical_misses) = (0, 0, 0, 0);
    // One slot per weight in the sweep: top-1, top-10, the share of answers that
    // stayed inside the fused top 10, and the queries fusion put first that the
    // text arm had not.
    let mut top1 = vec![0usize; IMAGE_WEIGHTS.len()];
    let mut top10 = vec![0usize; IMAGE_WEIGHTS.len()];
    let mut kept = vec![0usize; IMAGE_WEIGHTS.len()];
    let mut rescued = vec![0usize; IMAGE_WEIGHTS.len()];

    for (i, text) in texts.iter().enumerate().take(n) {
        // Each image's own OCR text is the query. It is the easiest possible
        // question and the same one the image-arm benchmark uses, so the two
        // numbers are directly comparable.
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

        // Both arms rank every file, so both of these resolve. A missing
        // position would mean an arm had dropped a file, which would make the
        // fusion below a different calculation than the one under test.
        let image_rank = image_ranks
            .iter()
            .position(|x| *x == i)
            .expect("the image arm ranks every file");
        let text_rank = text_ranks
            .iter()
            .position(|x| *x == i)
            .expect("the text arm ranks every file");
        let text_top: BTreeSet<usize> = text_ranks.iter().take(10).copied().collect();

        img_only += usize::from(image_rank == 0);
        txt_only += usize::from(text_rank == 0);
        scored += 1;

        // One fusion per weight, computed once and used for every column.
        let fusions: Vec<Vec<usize>> = IMAGE_WEIGHTS
            .iter()
            .map(|w_image| fuse(&image_ranks, &text_ranks, n, *w_image))
            .collect();

        // The queries the image arm could possibly help with: the ones the text
        // arm did not already put first. This is the denominator that decides
        // whether "the image arm adds something" is even an answerable question
        // on this corpus, so it is printed rather than assumed to be large.
        let lexical_missed = text_rank > 0;
        if lexical_missed {
            lexical_misses += 1;
        }

        for (slot, fused) in fusions.iter().enumerate() {
            top1[slot] += usize::from(fused[0] == i);
            top10[slot] += usize::from(fused.iter().take(10).any(|x| *x == i));
            // Regression guard: an answer the text arm had already found must
            // not be pushed out of the fused top 10 by an arm that is weaker on
            // this corpus.
            kept[slot] +=
                usize::from(text_top.contains(&i) && fused.iter().take(10).any(|x| *x == i));
            if lexical_missed {
                rescued[slot] += usize::from(fused[0] == i);
            }
        }
    }

    let d = scored.max(1) as f64;
    eprintln!("\n==== does OCR text rescue description search? ====");
    eprintln!("corpus                         {scored}");
    eprintln!("image arm alone (top-1)        {:.3}", img_only as f64 / d);
    eprintln!("OCR text alone (top-1)         {:.3}", txt_only as f64 / d);
    eprintln!(
        "queries the image arm could help   {lexical_misses}   \
         <- the ones the text arm did not already rank first"
    );

    eprintln!("\n-- the image arm's vote, swept --");
    eprintln!(
        "{:>8}  {:>7}  {:>7}  {:>7}  {:>10}",
        "w_image", "top-1", "top-10", "kept", "rescued"
    );
    for (slot, w_image) in IMAGE_WEIGHTS.iter().enumerate() {
        eprintln!(
            "{w_image:>8.2}  {:>7.3}  {:>7.3}  {:>7.3}  {:>4}/{:<5}",
            top1[slot] as f64 / d,
            top10[slot] as f64 / d,
            kept[slot] as f64 / d,
            rescued[slot],
            lexical_misses
        );
    }
    eprintln!(
        "\nw_image 0.00 is the lexical arm alone and 1.00 is the equal-weight fusion\n\
         ADR-0003 describes. `kept` is the share of queries whose answer stayed in\n\
         the fused top 10; `rescued` is how many of the queries above the text arm\n\
         missed at rank 1 were put first by fusion.\n\
         \n(8w / 5w query styles are computed but not scored here; the numbers above\n\
         use each image's full OCR text as its own query.)"
    );
}
