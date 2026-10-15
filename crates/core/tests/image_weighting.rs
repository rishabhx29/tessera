//! Does weighting the arms by how much text a file has preserve both?
//!
//! Ticket 10 was about to be built as equal-weight Reciprocal Rank Fusion, which
//! ticket 07 measured at **0.375** against **0.964** for the lexical arm alone on
//! real screenshots. This runs the correction *before* any of it is written into
//! the engine: the weights become a function of the file — `(1.0, 0.05)` where the
//! file has text, `(0.0, 1.0)` where it has none.
//!
//! It reports four rows, because the weighted one only means something against
//! all three of the others:
//!
//! - **lexical alone** — what the product has today
//! - **image alone** — the arm being added
//! - **weighted** — the correction
//! - **equal weight** — ADR-0003 as written, and the reason the correction exists
//!
//! `image_fusion` carries the full sweep of the image arm's vote and is where the
//! constant `0.05` comes from. This test pins the constants and the reason for
//! each; the sweep is the evidence.
//!
//! The two tests do not use the same corpus, and that is worth stating rather than
//! leaving to be discovered: `image_fusion` strides the folder down to 60 images
//! because it runs 14 weights over every query, and this one uses every image in
//! the folder that has text — 178 on the machine this was written, so its
//! denominators are three times larger. Neither is wrong; they answer the same
//! question from different directions, and this one can afford to be the larger.
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

/// How much each arm's vote is worth on a file that does have text.
///
/// `0.05` is the largest value measured to cost the lexical arm nothing. Over 56
/// real screenshots, each image's own OCR text as the query, the lexical arm
/// scores 0.964 alone, 0.964 at 0.05, and 0.857 at 0.10 — the value this constant
/// held before, on the endorsement of a smaller run of that corpus. Over the 178
/// images of this test the same comparison reads 0.584 alone, 0.579 at 0.05 and
/// 0.287 at equal weight: one query in 178, half a point against a standard error
/// of 3.7, so the shape of the result is unchanged on a corpus three times larger.
///
/// The image arm's *benefit* on files with text is neither measured nor claimed.
/// The 56-image sample gives the lexical arm only 2 misses, so nothing there could
/// show a second arm helping; this corpus misses 74 of 178, which is real
/// headroom, and 0.05 still does not beat the lexical arm alone on it. What is
/// measured is that this weight costs nothing measurable, and that is all the
/// constant is set from.
const WEIGHT_WITH_TEXT: (f32, f32) = (1.0, 0.05);
/// A file with no text has nothing for the lexical arm to match on, so it gets no
/// vote at all rather than a suppressed one.
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

    // The whole folder, with no cap. An earlier version of this took the first 40
    // when it meant to sample 40: the stride was `len / 40`, which is 1 for any
    // folder under 80 images, so the "sample" was a prefix with the corpus's own
    // size hidden inside it. Sampling is not needed here — one row per query is
    // cheap, unlike `image_fusion`'s fourteen weights — and the larger corpus is
    // the better denominator: 178 text-bearing queries rather than 56.
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
    eprintln!("corpus: {n} images, all of which have text");

    let (mut lexical_alone, mut image_alone, mut equal, mut weighted) = (0, 0, 0, 0);

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
        // `WEIGHT_WITHOUT_TEXT` is the branch that never fires here — which is the
        // honest statement about this corpus, and pinning it is what keeps a later
        // change to the no-text case from being silently unseen.
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

        lexical_alone += usize::from(lexical_ranked.first() == Some(&i));
        image_alone += usize::from(image_ranked.first() == Some(&i));
        equal += usize::from(fused(true).first() == Some(&i));
        weighted += usize::from(fused(false).first() == Some(&i));
    }

    let d = n.max(1) as f64;
    eprintln!("\n== weighting the arms, on files that all have text ==");
    eprintln!(
        "lexical arm alone                         {:.3}",
        lexical_alone as f64 / d
    );
    eprintln!(
        "image arm alone                           {:.3}",
        image_alone as f64 / d
    );
    eprintln!(
        "weighted RRF (the correction)             {:.3}",
        weighted as f64 / d
    );
    eprintln!(
        "equal-weight RRF (ADR-0003 as written)    {:.3}",
        equal as f64 / d
    );
    eprintln!(
        "\nThe weighted row is the point of the design, so it is reported against\n\
         the lexical arm rather than against equal weight alone. Whether the two\n\
         are equal exactly is left as a report and not an assertion: this corpus\n\
         is a folder of real screenshots on one machine, and an assertion on an\n\
         exact tie would be asserting a fixture this test does not have."
    );

    assert!(
        equal < lexical_alone,
        "equal-weight fusion was expected to be far worse than the lexical arm \
         alone on a corpus like this, and it was not ({equal} against \
         {lexical_alone}); if that is genuinely true the correction is \
         unnecessary and ticket 10 needs rewriting, not fixing"
    );
    assert!(
        weighted > equal,
        "down-weighting the image arm did not help, so the correction is wrong"
    );
}
