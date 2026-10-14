//! Diagnostic: *why* is description search at 27%?
//!
//! Three candidate explanations, and they need different fixes:
//!
//! 1. **The metric is wrong.** If the corpus contains near-duplicate screenshots
//!    — five Telegram chats differing only in a timestamp — then picking a
//!    different one of the five is not a retrieval failure, it is a tie the
//!    metric scores as a miss. A model at 95% and one at 30% can both look bad.
//! 2. **The model is wrong.** int8 quantisation, no attention mask, or the
//!    wrong pooled output.
//! 3. **The query is wrong.** OCR text is not what a person types. Measuring
//!    only the easy query overstates what a user will see; measuring only a
//!    paraphrase understates what the space can do.
//!
//! This measures all three so the next change is aimed at the right thing.
//!
//! ```
//! cargo test --release -p tessera-core --test image_diagnose -- --ignored --nocapture
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tessera_core::models::Models;
use tessera_core::vectors::siglip::SiglipEmbedder;
use tessera_core::vectors::{cosine, Embedding};

const DIM: usize = 768;

/// Two screenshots this similar are the same picture twice.
const DUPLICATE: f32 = 0.95;

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

fn screenshots_folder() -> PathBuf {
    std::env::var("TESSERA_SCREENSHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            PathBuf::from(home)
                .join("OneDrive")
                .join("Pictures")
                .join("Screenshots")
        })
}

fn screenshot_paths(folder: &Path, limit: usize) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(folder)
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
    found.sort();
    if found.len() > limit {
        let stride = found.len() / limit;
        found = (0..limit).map(|i| found[i * stride].clone()).collect();
    }
    found
}

/// Cluster near-identical screenshots so a tie between siblings is not scored
/// as a miss. Union-find over the similarity threshold.
fn clusters(embeddings: &[Embedding]) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..embeddings.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for i in 0..embeddings.len() {
        for j in (i + 1)..embeddings.len() {
            if cosine(&embeddings[i], &embeddings[j]) >= DUPLICATE {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                parent[a] = b;
            }
        }
    }
    (0..embeddings.len())
        .map(|i| find(&mut parent, i))
        .collect()
}

/// The part of an OCR string most likely to identify the screen: the words a
/// person would type to find it.
fn keywords(text: &str) -> String {
    text.split_whitespace()
        .filter(|w| w.len() > 3 && !w.chars().all(|c| c.is_ascii_digit() || c == ':'))
        .take(12)
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
#[ignore = "needs the 213 MB siglip model set"]
fn diagnose_the_27_percent() {
    let models = models();
    assert!(
        SiglipEmbedder::is_installed(&models),
        "run: tessera models install --set siglip"
    );
    let embedder = SiglipEmbedder::new(&models, 4).expect("embedder");
    let folder = screenshots_folder();
    assert!(
        folder.is_dir(),
        "no screenshot folder at {}",
        folder.display()
    );

    let paths = screenshot_paths(&folder, 80);
    eprintln!("diagnosing over {} images", paths.len());

    let ocr = tessera_core::ocr::select(&tessera_core::ocr::OcrChoice::Native, &models)
        .expect("an OCR engine");

    let mut images: Vec<Embedding> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    for path in &paths {
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
    eprintln!("{} images with readable text", images.len());

    // ---- 1. How confusable is this corpus at all? -------------------------
    let groups = clusters(&images);
    let mut sizes: HashMap<usize, usize> = HashMap::new();
    for g in &groups {
        *sizes.entry(*g).or_default() += 1;
    }
    let distinct = sizes.len();
    let largest = sizes.values().copied().max().unwrap_or(0);
    let duplicate_fraction = 1.0 - (distinct as f64 / images.len().max(1) as f64);
    eprintln!(
        "\n== corpus shape ==\n{} images collapse into {distinct} distinct pictures \
         ({:.0}% are near-duplicates); largest cluster is {largest}",
        images.len(),
        duplicate_fraction * 100.0
    );

    // ---- 2. Three query styles, three numbers. ---------------------------
    /// A query builder: OCR text in, the text actually sent to the model out.
    type QueryStyle<'a> = (&'a str, &'a dyn Fn(&str) -> String);
    let styles: Vec<QueryStyle<'_>> = vec![
        ("full OCR text", &|t: &str| t.to_string()),
        ("first 8 words", &|t: &str| {
            t.split_whitespace().take(8).collect::<Vec<_>>().join(" ")
        }),
        ("keywords only", &keywords),
    ];
    for (label, build) in styles {
        let (mut top1, mut top1_distinct, mut in5, mut reciprocal, mut ties) =
            (0usize, 0usize, 0usize, 0f64, 0usize);

        for (i, text) in texts.iter().enumerate() {
            let query_text = build(text);
            if query_text.trim().is_empty() {
                continue;
            }
            let Ok(query) = embedder.embed_text(&query_text, DIM) else {
                continue;
            };
            let mut scored: Vec<(usize, f32)> = images
                .iter()
                .enumerate()
                .map(|(j, e)| (j, cosine(&query, e)))
                .collect();
            scored.sort_by(|a, b| b.1.total_cmp(&a.1));

            let rank = scored.iter().position(|(j, _)| *j == i).unwrap_or(0);
            if rank == 0 {
                top1 += 1;
            }
            if rank < 5 {
                in5 += 1;
            }
            reciprocal += 1.0 / (rank + 1) as f64;

            // "Distinct top-1": correct if the winner is the same *picture*,
            // i.e. in the same near-duplicate cluster as the target.
            let winner_cluster = groups[scored[0].0];
            if winner_cluster == groups[i] {
                top1_distinct += 1;
            }
            // Count genuine ties: the runner-up within a hair of the winner.
            if scored.len() > 1 && scored[0].1 - scored[1].1 < 0.01 {
                ties += 1;
            }
        }

        let n = texts.len().max(1) as f64;
        eprintln!(
            "\n== query style: {label} ==\ntop-1            {:.3}\ntop-1 (distinct)  {:.3}\n\
             recall@5        {:.3}\nMRR              {:.3}\nnear-ties        {ties}",
            top1 as f64 / n,
            top1_distinct as f64 / n,
            in5 as f64 / n,
            reciprocal / n,
        );
    }

    // ---- 3. Is the embedding itself degenerate? --------------------------
    let norms: Vec<f32> = images.iter().map(|e| cosine(e, e)).collect();
    let mut all: Vec<f32> = Vec::new();
    for i in 0..images.len() {
        for j in (i + 1)..images.len() {
            all.push(cosine(&images[i], &images[j]));
        }
    }
    all.sort_by(|a, b| a.total_cmp(b));
    let median = all[all.len() / 2];
    let p95 = all[all.len() * 95 / 100];
    eprintln!(
        "\n== embedding sanity ==\nmedian pairwise cosine {median:.3}, p95 {p95:.3}\n\
         (a flat distribution around 0 would mean the int8 export lost the space)"
    );
    debug_assert!(norms.iter().all(|n| (*n - 1.0).abs() < 1e-4));
}
