//! Ticket 10 acceptance, on real models and a real folder: one search box.
//!
//! The fuse arithmetic, the query parser and the orchestration over a real
//! store are tested without ONNX in the unit tests. This file is the demo half:
//! a folder with several formats in it gets indexed once, and then one query —
//! words and filters in the same bar — comes back with hits that name the arm
//! or arms that found them.
//!
//! Both tests are ignored because they need the installed models and a real
//! corpus, which CI does not have; skipping is printed, never silent. Run them
//! in release — debug builds spend more time hashing the pinned model files
//! than searching:
//!
//! ```text
//! cargo test --release -p tessera-core --test fusion_search -- --ignored --nocapture
//! ```
//!
//! - `TESSERA_FUSION_CORPUS` — the folder to index. Defaults to the
//!   screenshots folder `image_fusion.rs` measures on.
//! - `TESSERA_FUSION_INDEX` — where to build the index. Defaults to a temp
//!   dir; point it somewhere stable to keep the OCR and embedding work.
//! - `TESSERA_FUSION_QUERY` — semicolon-separated queries, for asking about
//!   your own corpus. Defaults to eight generic ones.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tessera_core::fusion::{parse_query, search};
use tessera_core::index::{
    docx::DocxPipeline, image::ImagePipeline, index_folder_with_text, pdf::PdfPipeline,
    text::TextPipeline, Pipeline,
};
use tessera_core::models::Models;
use tessera_core::ocr::{self, OcrChoice};
use tessera_core::store::Store;
use tessera_core::vectors::minilm::MiniLm;
use tessera_core::vectors::siglip::SiglipEmbedder;
use tessera_core::vectors::{QueryEmbedder, TextEmbedder};

/// The models directory, from `TESSERA_MODELS_DIR` or the default beside the
/// index — the same place the CLI looks.
fn models() -> Models {
    let dir = match std::env::var("TESSERA_MODELS_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => tessera_core::cli::models_dir(
            &tessera_core::cli::default_data_dir().expect("a data dir"),
            None,
        ),
    };
    Models::open(&dir).expect("the models dir is openable")
}

/// The folder the demo indexes.
fn corpus_folder() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("TESSERA_FUSION_CORPUS") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let shots = PathBuf::from(home)
        .join("OneDrive")
        .join("Pictures")
        .join("Screenshots");
    shots.is_dir().then_some(shots)
}

/// The arm-side embedders, or `None` with an explanation when the meaning arm
/// cannot run at all. Skipping is stated, never silent.
fn embedders(models: &Models) -> Option<(Option<Arc<SiglipEmbedder>>, MiniLm)> {
    if !MiniLm::is_installed(models) {
        eprintln!(
            "skipping: the minilm models are not installed. Run: tessera models install --set minilm"
        );
        return None;
    }
    let minilm = MiniLm::new(models, 4).expect("the minilm models are installed and load");
    let siglip = if SiglipEmbedder::is_installed(models) {
        Some(Arc::new(
            SiglipEmbedder::new(models, 4).expect("the siglip models are installed and load"),
        ))
    } else {
        eprintln!(
            "note: the siglip models are not installed, so the image arm will have nothing to \
             rank. Run: tessera models install --set siglip"
        );
        None
    };
    Some((siglip, minilm))
}

fn queries() -> Vec<String> {
    std::env::var("TESSERA_FUSION_QUERY")
        .map(|raw| raw.split(';').map(|q| q.trim().to_string()).collect())
        .unwrap_or_else(|_| {
            vec![
                "invoice".to_string(),
                "a chart about flight delays".to_string(),
                "the meeting notes about the budget".to_string(),
                "http response headers".to_string(),
                "error handling examples".to_string(),
                "the contract with the non-compete".to_string(),
                "security considerations".to_string(),
                "the spec about quality of service".to_string(),
            ]
        })
}

#[test]
#[ignore = "demo: needs the siglip and minilm models and a real folder; run in release"]
fn a_real_folder_answers_one_query_with_every_arm() {
    let models = models();
    let Some(corpus) = corpus_folder() else {
        eprintln!(
            "skipping: no corpus folder. Set TESSERA_FUSION_CORPUS to a folder to index and \\
             search."
        );
        return;
    };
    let Some((siglip, minilm)) = embedders(&models) else {
        return;
    };

    let temp = tempfile::tempdir().unwrap();
    let data: PathBuf = match std::env::var("TESSERA_FUSION_INDEX") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => temp.path().to_path_buf(),
    };
    let store = Store::open(&data).unwrap();

    // Every format the engine has, and the two embedders on top: the same
    // pipelines `tessera index` builds when the models are installed.
    let engine = ocr::select(&OcrChoice::Native, &models).expect("an engine selection");
    let pipelines: Vec<Box<dyn Pipeline>> = vec![
        Box::new(TextPipeline),
        Box::new(PdfPipeline),
        Box::new(DocxPipeline),
        Box::new(ImagePipeline::new(engine.engine).with_embedder(siglip.clone())),
    ];

    let started = Instant::now();
    let summary = index_folder_with_text(&store, &pipelines, &corpus, &minilm).unwrap();
    println!(
        "indexed {} file(s) from {} in {:.1} s: {} unchanged, {} moved, {} skipped, {} failed, \
         {} awaiting OCR",
        summary.indexed,
        corpus.display(),
        started.elapsed().as_secs_f64(),
        summary.unchanged,
        summary.moved,
        summary.skipped,
        summary.failed,
        summary.awaiting_ocr,
    );
    assert!(
        summary.indexed + summary.unchanged > 0,
        "the corpus folder held no readable files"
    );
    println!(
        "index vectors: {} chunk(s), {} image(s)",
        store.text_embedding_count().unwrap(),
        store.image_embedding_count().unwrap()
    );

    let image: Option<&dyn QueryEmbedder> = siglip.as_deref().map(|e| e as &dyn QueryEmbedder);
    let text: &dyn TextEmbedder = &minilm;

    let mut named: Vec<&'static str> = Vec::new();
    for query in queries() {
        let parsed = parse_query(std::slice::from_ref(&query)).unwrap();
        let outcome = search(&data, &parsed, image, Some(text), 5).unwrap();
        println!("\n-- {query:?} --");
        for hit in &outcome.hits {
            named.extend(hit.arms.names());
            let page = hit.page.map(|p| format!(", page {p}")).unwrap_or_default();
            println!(
                "{} [{}] ({}{page})",
                hit.path,
                if hit.arms.is_empty() {
                    "no arm".to_string()
                } else {
                    hit.arms.names().join(", ")
                },
                hit.kind,
            );
        }
        if outcome.hits.is_empty() {
            println!("  (no hits)");
        }
        for note in &outcome.notes {
            println!("  note: {note}");
        }
    }
    named.sort_unstable();
    named.dedup();
    println!("\narms that answered: {named:?}");
    assert!(!named.is_empty(), "no arm answered any query");

    // A filter with no terms is a listing, not a search: files of that kind,
    // newest first, wearing no arm label because no arm ran.
    let parsed = parse_query(&["type:image".to_string()]).unwrap();
    let listing = search(&data, &parsed, image, Some(text), 50).unwrap();
    assert!(listing.timings.is_empty(), "a listing runs no arms");
    assert!(
        listing.hits.iter().all(|hit| hit.kind == "image"),
        "a type:image listing returned something else: {:?}",
        listing
            .hits
            .iter()
            .map(|hit| (&hit.path, &hit.kind))
            .collect::<Vec<_>>()
    );
    println!("type:image listing: {} file(s)", listing.hits.len());
    if siglip.is_some() && !listing.hits.is_empty() {
        assert!(
            store.image_embedding_count().unwrap() > 0,
            "images are indexed but the image arm has no vectors to rank"
        );
    }
}

#[test]
#[ignore = "measurement: needs an existing index (TESSERA_FUSION_INDEX) and the installed \
            models; run in release with --nocapture"]
fn fused_search_latency_is_measured() {
    let models = models();
    let Ok(index) = std::env::var("TESSERA_FUSION_INDEX") else {
        eprintln!(
            "skipping: set TESSERA_FUSION_INDEX to a data dir holding an index first, e.g. \
             `tessera --data-dir <dir> index <folder>`, then re-run."
        );
        return;
    };
    let index = PathBuf::from(index);
    let Some((siglip, minilm)) = embedders(&models) else {
        return;
    };
    let image: Option<&dyn QueryEmbedder> = siglip.as_deref().map(|e| e as &dyn QueryEmbedder);
    let text: &dyn TextEmbedder = &minilm;

    let store = Store::open(&index).unwrap();
    let chunks = store.text_embedding_count().unwrap();
    let images = store.image_embedding_count().unwrap();
    println!("index: {chunks} chunk vector(s), {images} image vector(s)");

    let queries = queries();
    let run = |query: &str| -> f64 {
        let args = [query.to_string()];
        let parsed = parse_query(&args).unwrap();
        let started = Instant::now();
        let outcome = search(&index, &parsed, image, Some(text), 10).unwrap();
        assert!(
            !outcome.hits.is_empty() || !outcome.notes.is_empty(),
            "a query on the measured index found nothing and said nothing: {query:?}"
        );
        started.elapsed().as_secs_f64() * 1000.0
    };

    // One warm-up call per query: the first includes the query planner's cold
    // start and the page cache warming, which are per-process costs, not the
    // search's.
    for query in &queries {
        run(query);
    }

    let mut samples: Vec<f64> = Vec::new();
    let mut per_arm: Vec<(&'static str, Vec<f64>)> = Vec::new();
    for i in 0..40 {
        let query = &queries[i % queries.len()];
        let parsed = parse_query(std::slice::from_ref(query)).unwrap();
        let started = Instant::now();
        let outcome = search(&index, &parsed, image, Some(text), 10).unwrap();
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
        for timing in &outcome.timings {
            match per_arm
                .iter_mut()
                .find(|(arm, _)| *arm == timing.arm.name())
            {
                Some((_, list)) => list.push(timing.millis),
                None => per_arm.push((timing.arm.name(), vec![timing.millis])),
            }
        }
    }

    let at = |sorted: &[f64], p: f64| sorted[((sorted.len() - 1) as f64 * p).round() as usize];
    let report = |label: &str, mut values: Vec<f64>| {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        println!(
            "{label:<9} p50 {:>6.1} ms   p95 {:>6.1} ms   mean {:>6.1} ms   n {}",
            at(&values, 0.5),
            at(&values, 0.95),
            mean,
            values.len()
        );
    };
    println!("\n-- fused search, one query in, one ranking out --");
    report("total", samples.clone());
    for (arm, values) in per_arm {
        report(arm, values);
    }
    assert_eq!(samples.len(), 40);

    // The arms this run actually had. A machine without the siglip set is
    // measured with two arms, and the line says so rather than pretending.
    println!(
        "\narms: {}",
        if siglip.is_some() {
            "keyword, image, meaning"
        } else {
            "keyword, meaning"
        }
    );
}
