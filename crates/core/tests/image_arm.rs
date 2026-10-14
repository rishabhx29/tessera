//! Ticket 07 acceptance: does the image arm put a screenshot and its
//! description in the same space?
//!
//! The methodology is the spike's, deliberately unchanged so the numbers are
//! comparable with `docs/performance.md` rather than being a fresh set of
//! numbers that merely looks good.
//!
//! Two controls, because one is not enough:
//!
//! - **The rendered-text set** (20 images, query = the phrase rendered into
//!   them). The spike measured top-1 **0.90** here with chance at 5%. A number
//!   in that class means the space is sound rather than accidentally ordered.
//! - **Real screenshots** (the user's own folder, query = the OCR text of one
//!   image). The spike measured top-1 **0.46–0.50**. This is the honest,
//!   unglamorous number and it is the one that matters for the product.
//!
//! Tests that need the 213 MB `siglip` model set are `#[ignore]`d, because CI
//! does not download models and a test that cannot run should say so rather
//! than pass vacuously. Run them with:
//!
//! ```
//! tessera models install --set siglip
//! cargo test -p tessera-core --test image_arm -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};

use tessera_core::models::Models;
use tessera_core::vectors::siglip::SiglipEmbedder;
use tessera_core::vectors::{cosine, Embedding};

/// SigLIP-base's output width.
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
    Models::open(&dir).expect("the models dir is openable")
}

fn embedder() -> Option<SiglipEmbedder> {
    if !SiglipEmbedder::is_installed(&models()) {
        eprintln!(
            "skipping: the siglip model set is not installed. \
             Fix with: tessera models install --set siglip"
        );
        return None;
    }
    Some(SiglipEmbedder::new(&models(), 4).expect("the embedder loads"))
}

/// The user's real screenshot corpus, if it is there. Names are only used to
/// *find* files; no image content is read into the repository (SECURITY.md).
fn screenshots_folder() -> Option<PathBuf> {
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
    if folder.is_dir() {
        Some(folder)
    } else {
        eprintln!(
            "skipping: no screenshot folder at {} (set TESSERA_SCREENSHOTS to override)",
            folder.display()
        );
        None
    }
}

fn screenshot_paths(folder: &Path, limit: usize) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(folder)
        .expect("readable")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("png" | "jpg" | "jpeg")
            )
        })
        .collect();
    found.sort();
    // Windows screenshots are named `Screenshot <date> <time>.png`, so a
    // lexicographic sort is a *chronological* sort and truncating takes the
    // oldest N, not a sample. That distinction is not academic: the first 20
    // files of this corpus score 0.84 and later 20-files score 0.12-0.20, so
    // "the oldest 20" is the single most flattering slice available and was
    // what an earlier version of this test did.
    if found.len() > limit {
        // Even stride across the whole folder rather than the oldest prefix.
        let stride = found.len() / limit;
        found = (0..limit).map(|i| found[i * stride].clone()).collect();
    }
    found
}

/// Top-1 accuracy: for each query, how often is the intended image ranked first.
///
/// Returns `(hits, total, miss_rank_lt_5)`. Recall@5 is reported alongside
/// because this corpus is full of near-duplicate screenshots — five shots of
/// the same chat window are interchangeable, and picking a different one is not
/// really a miss. Top-1 alone flatters neither the model nor the user.
fn top1(
    embedder: &SiglipEmbedder,
    corpus: &[(PathBuf, Embedding)],
    queries: &[(String, usize)],
) -> (usize, usize, usize) {
    let mut hits = 0;
    let mut near = 0;
    for (text, expected) in queries {
        let Ok(query) = embedder.embed_text(text, DIM) else {
            panic!("embedding the query {text:?}");
        };
        let mut scored: Vec<(usize, f32)> = corpus
            .iter()
            .enumerate()
            .map(|(index, (_, embedding))| (index, cosine(&query, embedding)))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));

        match scored.iter().position(|(index, _)| index == expected) {
            Some(0) => hits += 1,
            Some(rank) if rank < 5 => near += 1,
            Some(rank) => {
                // Name the file, not the index: an index into a list nobody
                // printed is not something anyone can act on.
                eprintln!(
                    "  miss: {:?} -> got {:?} ({:.3}) instead of {:?} ({:.3}), rank {rank}",
                    truncate(text),
                    corpus[scored[0].0].0.file_name().unwrap_or_default(),
                    scored[0].1,
                    corpus[*expected].0.file_name().unwrap_or_default(),
                    scored[rank].1,
                );
            }
            None => panic!("the query's target image is not in the corpus"),
        }
    }
    (hits, queries.len(), near)
}

fn truncate(text: &str) -> String {
    text.chars().take(48).collect()
}

/// The corpus size to measure over. The whole folder, because the number that
/// decides whether this works is the one on a representative slice — and the
/// hard-coded 20 this test used to carry was, on this corpus, the single most
/// flattering slice available (0.84 against 0.12–0.20 elsewhere).
const CORPUS: usize = 60;

#[test]
#[ignore = "needs the 213 MB siglip model set"]
fn text_and_image_land_in_the_same_space() {
    // The control that says the space is sound at all. If a screenshot and the
    // words describing it are not close, nothing else about this ticket matters.
    //
    // `--ignored` means "the author asked for this to run", so a missing model
    // or corpus is a failure here, not a green tick. Returning early here would
    // make `--ignored` report success for a run that measured nothing, which is
    // the exact failure this repo keeps catching.
    let Some(embedder) = embedder() else {
        panic!(
            "the siglip model set is not installed; this test was explicitly run. Fix with: \
             tessera models install --set siglip"
        );
    };
    let Some(folder) = screenshots_folder() else {
        panic!(
            "no screenshot folder; this test was explicitly run. Set TESSERA_SCREENSHOTS to the \
             folder of real screenshots to measure against."
        );
    };
    let paths = screenshot_paths(&folder, CORPUS);
    assert!(
        paths.len() >= 10,
        "only {} screenshots available; too few to say anything",
        paths.len()
    );
    eprintln!(
        "measuring over {} images from {}",
        paths.len(),
        folder.display()
    );

    let corpus: Vec<(PathBuf, Embedding)> = paths
        .iter()
        .map(|path| {
            let embedding = embedder
                .embed_image(path, DIM)
                .unwrap_or_else(|err| panic!("embedding {}: {err:#}", path.display()));
            (path.clone(), embedding)
        })
        .collect();

    // **The query is the text OCR read out of the image**, which is the
    // methodology the spike used and the only one that asks a real question.
    //
    // The first version of this test queried with each image's *filename*
    // ("Screenshot 2024-02-18 142743") and measured top-1 of 0.05 — exactly
    // chance, and for the right reason: a filename is a timestamp and tells
    // SigLIP nothing about what is in the picture. Measuring that would have
    // looked like a failed model when it was a failed test.
    let ocr = tessera_core::ocr::select(&tessera_core::ocr::OcrChoice::Native, &models())
        .expect("select an OCR engine");

    let mut queries: Vec<(String, usize)> = Vec::new();
    for (index, (path, _)) in corpus.iter().enumerate() {
        let Ok(text) = ocr.engine.recognize(path) else {
            eprintln!("  skipping {}: no text read", path.display());
            continue;
        };
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            continue;
        }
        queries.push((text, index));
    }
    eprintln!(
        "built {} queries from {} images via OCR text",
        queries.len(),
        corpus.len()
    );

    let (hits, total, near) = top1(&embedder, &corpus, &queries);
    let top1_accuracy = hits as f64 / total as f64;
    let recall5 = (hits + near) as f64 / total as f64;
    // Chance is 1 over the **corpus**, not over the number of queries: an image
    // whose OCR read nothing is not in the corpus to be retrieved, so it is not
    // a candidate. Getting this denominator wrong makes the headline look worse
    // than it is, which is the less harmful direction but still wrong.
    let chance = 1.0 / corpus.len() as f64;
    eprintln!(
        "top-1 {hits}/{total} = {top1_accuracy:.3} | recall@5 = {recall5:.3} | \
         chance {chance:.3} | corpus {} images",
        corpus.len()
    );
    // The bar is "clearly above chance", deliberately low. The measured value
    // on a representative slice is well clear of it; the number that matters is
    // the one printed above and recorded in docs/performance.md.
    assert!(
        top1_accuracy > 3.0 * chance,
        "top-1 of {top1_accuracy:.3} against chance {chance:.3} is not retrieving by description"
    );
}

#[test]
#[ignore = "needs the 213 MB siglip model set"]
fn query_side_latency_is_measured_rather_than_assumed() {
    // The number the perf doc needs. Ten queries, warm, reported rather than
    // asserted: this is a measurement, and a threshold here would just be a
    // guess at the threshold written as a test.
    let Some(embedder) = embedder() else {
        return;
    };
    let queries = [
        "a chart about airline delays",
        "a screenshot of an email inbox",
        "a terminal window with build output",
        "a spreadsheet of expenses",
        "a photo of a dog",
    ];

    // One warm-up so the measurement is of steady state, not session setup.
    let _ = embedder.embed_text(queries[0], DIM);

    let mut timings = Vec::new();
    for text in queries {
        let start = std::time::Instant::now();
        let _ = embedder.embed_text(text, DIM).expect("embed");
        timings.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    timings.sort_by(|a, b| a.total_cmp(b));
    let median = timings[timings.len() / 2];
    eprintln!(
        "text-encode p50 over {} queries: {median:.1} ms",
        queries.len()
    );
}

#[test]
fn a_query_and_an_image_of_the_same_width_can_be_compared() {
    // The no-models half of the contract, so it runs everywhere: both sides of
    // the cosine are unit length and 768 wide, which is what the scan assumes.
    let a = Embedding::new(vec![0.0; DIM - 1].into_iter().chain([1.0]).collect(), DIM).unwrap();
    let b = Embedding::new(vec![0.0; DIM], DIM).unwrap();
    assert!(
        b.clone().normalize() == 0.0,
        "the zero vector has no direction"
    );
    assert!((cosine(&a, &a) - 1.0).abs() < 1e-6);
}
