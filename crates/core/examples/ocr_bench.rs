//! Measure the OCR engine over a real screenshot corpus.
//!
//! Run it, record the numbers with the hardware and the corpus, and put them in
//! `docs/performance.md`:
//!
//! ```text
//! cargo run --release --example ocr_bench -- <folder> [count]
//! ```
//!
//! Reports p50 and p95 per image, the mean characters extracted, and how many
//! images yielded nothing at all — the last one matters because "fast" and
//! "returned nothing" look identical in a latency table.
//!
//! It prints **no recognised text**. Screenshots are private by default and the
//! numbers are the only thing this project is allowed to publish
//! (SECURITY.md).

use std::path::{Path, PathBuf};
use std::time::Instant;

use tessera_core::index::image::claims_extension;
use tessera_core::ocr::OcrChoice;

/// Extensions that are screenshots rather than photographs, because a photo has
/// no text in it and would flatter the numbers. Format detection is the
/// pipeline's own, so the benchmark measures the corpus the engine would
/// actually index.
const SCREENSHOT_HINTS: [&str; 4] = ["screenshot", "screen shot", "snip", "capture"];

fn find_screenshots(folder: &Path, limit: usize) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!("cannot read {}: {err}", folder.display());
            return found;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        let extension = path.extension().unwrap_or_default().to_string_lossy();
        if claims_extension(&extension) && SCREENSHOT_HINTS.iter().any(|hint| name.contains(hint)) {
            found.push(path);
        }
    }
    found.sort();
    found.truncate(limit);
    found
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p / 100.0) * (sorted.len() - 1) as f64;
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    if low == high {
        return sorted[low];
    }
    sorted[low] + (sorted[high] - sorted[low]) * (rank - low as f64)
}

/// Where the models live: the same place the CLI looks, so the benchmark
/// measures the engine the indexer would actually use. `TESSERA_MODELS_DIR`
/// overrides it.
fn models_dir() -> tessera_core::models::Models {
    let dir = match std::env::var("TESSERA_MODELS_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => tessera_core::cli::models_dir(
            &tessera_core::cli::default_data_dir().expect("a data dir"),
            None,
        ),
    };
    tessera_core::models::Models::open(&dir).expect("the models dir is openable")
}

fn main() {
    let mut args = std::env::args().skip(1);
    let folder = match args.next() {
        Some(folder) => PathBuf::from(folder),
        None => {
            eprintln!("usage: ocr_bench <folder> [engine] [count]");
            std::process::exit(2);
        }
    };
    // Which engine, so both can be measured on one corpus in one session: a
    // comparison across different machines or different days is not one.
    let engine = args.next().unwrap_or_else(|| "native".to_string());
    let choice = match engine.as_str() {
        "native" => OcrChoice::Native,
        other => OcrChoice::Named(other.to_string()),
    };
    let limit: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(40);

    let selection = match tessera_core::ocr::select(&choice, &models_dir()) {
        Ok(selection) => selection,
        Err(err) => {
            eprintln!("no OCR engine: {err:#}");
            std::process::exit(1);
        }
    };
    let files = find_screenshots(&folder, limit);
    if files.is_empty() {
        eprintln!("no screenshots found in {}", folder.display());
        std::process::exit(1);
    }

    println!("engine: {}", selection.name);
    println!("corpus: {} images from {}", files.len(), folder.display());

    let mut timings = Vec::with_capacity(files.len());
    let mut chars = Vec::with_capacity(files.len());
    let mut empty = 0usize;
    // One warm-up run: the first call pays for JIT-free paths, WinRT
    // initialisation, and cold file cache, and none of that is what we are
    // measuring.
    if let Err(err) = selection.engine.recognize(&files[0]) {
        eprintln!("warm-up failed on {}: {err:#}", files[0].display());
        std::process::exit(1);
    }

    for path in &files {
        let started = Instant::now();
        match selection.engine.recognize(path) {
            Ok(text) => {
                timings.push(started.elapsed().as_secs_f64() * 1000.0);
                let len = text.trim().chars().count();
                chars.push(len);
                if len == 0 {
                    empty += 1;
                }
            }
            Err(err) => eprintln!("failed: {}: {err:#}", path.display()),
        }
    }
    if timings.is_empty() {
        eprintln!("no image was readable");
        std::process::exit(1);
    }
    timings.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean: f64 = timings.iter().sum::<f64>() / timings.len() as f64;
    let mean_chars: f64 = chars.iter().sum::<usize>() as f64 / chars.len() as f64;

    println!("images read: {}", timings.len());
    println!("p50: {:.1} ms", percentile(&timings, 50.0));
    println!("p95: {:.1} ms", percentile(&timings, 95.0));
    println!("mean: {:.1} ms", mean);
    println!("mean chars: {mean_chars:.0}");
    println!("images with no text: {empty}");
}
