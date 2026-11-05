//! Ticket 15 diagnosis: split a query's milliseconds into model and scan.
//!
//! The gate's own arm timings say the meaning arm costs 310 ms at 5,000 files,
//! but an arm is a model call plus a scan and the two need different fixes. The
//! tests here measure the scan half on an existing index with a fixed query
//! vector, so no ONNX session — and therefore no model warm-up, thread pool or
//! CPU contention — is involved. Run the ignored measurement in `text_arm.rs`
//! for the embedding half.
//!
//! ```text
//! $env:TESSERA_QUERY_INDEX = "$env:TEMP\tessera-gate15\index"
//! cargo test --release -p tessera-core --test query_latency -- --ignored --nocapture
//! ```
//!
//! The fused case uses a fixed-vector [`TextEmbedder`], so it measures the
//! orchestration, evidence, fusion and display passes without a model. The real
//! fused query is measured by `fusion_search.rs` with the installed models.
//!
//! Every test skips out loud when `TESSERA_QUERY_INDEX` is unset: a green run
//! that measured nothing is the failure mode this file exists to avoid.

use std::path::PathBuf;
use std::time::Instant;

use tessera_core::fusion::{parse_query, search};
use tessera_core::store::{Filter, Store, TEXT_EMBEDDING_DIM};
use tessera_core::vectors::{rank, Embedding, TextEmbedder};

fn index_dir() -> Option<PathBuf> {
    match std::env::var("TESSERA_QUERY_INDEX") {
        Ok(dir) => Some(PathBuf::from(dir)),
        Err(_) => {
            eprintln!(
                "skipping: set TESSERA_QUERY_INDEX to a data dir holding an index, e.g. \
                 `tessera --data-dir <dir> index <folder>`, then re-run."
            );
            None
        }
    }
}

/// A deterministic unit vector, so the scan is measured without a model.
///
/// The values are arbitrary but fixed and non-degenerate: a zero vector would
/// make every cosine zero, and a sort of equal scores is a different (and
/// unusually cheap) path through the ranking.
fn query_vector() -> Embedding {
    let raw: Vec<f32> = (0..TEXT_EMBEDDING_DIM)
        .map(|i| ((i * 37 % 101) as f32) - 50.0)
        .collect();
    Embedding::new(raw, TEXT_EMBEDDING_DIM)
        .expect("the fixed vector has the model's width")
        .normalized()
}

fn percentiles(label: &str, mut values: Vec<f64>) {
    assert!(!values.is_empty(), "no samples for {label}");
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    println!(
        "{label:<16} p50 {:>7.1} ms   p95 {:>7.1} ms   mean {:>7.1} ms   n {}",
        at(0.5),
        at(0.95),
        mean,
        values.len()
    );
}

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

/// The scan half of the meaning arm, stage by stage.
#[test]
#[ignore = "measurement: needs an existing index (TESSERA_QUERY_INDEX); run in release"]
fn text_scan_stages_are_measured() {
    let Some(dir) = index_dir() else {
        return;
    };
    let store = Store::open(&dir).expect("the index is openable");
    let chunks = store.text_embedding_count().expect("the vector count");
    println!("index: {chunks} text chunk vector(s)");

    let query = query_vector();
    let dim = TEXT_EMBEDDING_DIM;

    // One warm-up pass over every path: the first read is cold page cache and
    // the first statement is cold SQLite preparation, per-process costs that a
    // query in a running app does not pay.
    let _ = store.text_embeddings(dim).expect("a warm-up read");
    let _ = store
        .ranked_text_file_tiles(&query, dim)
        .expect("a warm-up scan");
    let _ = store
        .describe_text(&query, dim, 10)
        .expect("a warm-up describe");

    const REPS: usize = 12;
    let mut read = Vec::with_capacity(REPS);
    let mut rank_only = Vec::with_capacity(REPS);
    let mut scan = Vec::with_capacity(REPS);
    let mut describe = Vec::with_capacity(REPS);

    for _ in 0..REPS {
        let started = Instant::now();
        let embeddings = store.text_embeddings(dim).expect("the vectors read");
        read.push(ms(started));

        let started = Instant::now();
        let scored = rank(
            &query,
            embeddings.iter().map(|(id, e)| (*id, e)).collect(),
            embeddings.len(),
        );
        rank_only.push(ms(started));
        assert_eq!(scored.len(), embeddings.len(), "the scan lost candidates");

        let started = Instant::now();
        let ranked = store
            .ranked_text_file_tiles(&query, dim)
            .expect("the arm ranks");
        scan.push(ms(started));
        assert!(!ranked.is_empty(), "the index holds vectors but no files");

        let started = Instant::now();
        let hits = store
            .describe_text(&query, dim, 10)
            .expect("describe_text ranks");
        describe.push(ms(started));
        assert!(!hits.is_empty(), "describe_text found nothing");
    }

    println!("\n-- meaning arm, fixed query vector, no model --");
    percentiles("decode (read)", read);
    percentiles("rank (already read)", rank_only);
    percentiles("ranked_text_file_tiles", scan);
    percentiles("describe_text(10)", describe);
}

/// The image arm's scan, with the same fixed query.
#[test]
#[ignore = "measurement: needs an existing index (TESSERA_QUERY_INDEX); run in release"]
fn image_scan_stages_are_measured() {
    let Some(dir) = index_dir() else {
        return;
    };
    let store = Store::open(&dir).expect("the index is openable");
    let images = store.image_embedding_count().expect("the vector count");
    println!("index: {images} image vector(s)");
    if images == 0 {
        eprintln!("skipping: the index holds no image vectors");
        return;
    }

    let dim = tessera_core::store::IMAGE_EMBEDDING_DIM;
    let query = Embedding::new(
        (0..dim).map(|i| ((i * 37 % 101) as f32) - 50.0).collect(),
        dim,
    )
    .expect("the fixed vector has the image model's width")
    .normalized();

    let _ = store.image_embeddings(dim).expect("a warm-up read");
    let _ = store
        .ranked_image_file_ids(&query, dim)
        .expect("a warm-up scan");

    const REPS: usize = 20;
    let mut read = Vec::with_capacity(REPS);
    let mut scan = Vec::with_capacity(REPS);
    for _ in 0..REPS {
        let started = Instant::now();
        let embeddings = store.image_embeddings(dim).expect("the vectors read");
        read.push(ms(started));

        let started = Instant::now();
        let ranked = store
            .ranked_image_file_ids(&query, dim)
            .expect("the arm ranks");
        scan.push(ms(started));
        assert_eq!(ranked.len(), embeddings.len(), "the scan lost candidates");
    }

    println!("\n-- image arm, fixed query vector, no model --");
    percentiles("decode (read)", read);
    percentiles("ranked_image_ids", scan);
}

/// A text embedder that returns one fixed vector: the fused path without ONNX.
struct FixedText(Embedding);

impl TextEmbedder for FixedText {
    fn version(&self) -> &str {
        "query-latency-fixed-vector"
    }

    fn embed_text(&self, _text: &str) -> anyhow::Result<Embedding> {
        Ok(self.0.clone())
    }
}

/// The fused path's own passes — evidence, fusion, display — with the text
/// arm's model removed. The difference from `describe_text` is what fusion
/// costs after the scan has answered.
#[test]
#[ignore = "measurement: needs an existing index (TESSERA_QUERY_INDEX); run in release"]
fn fused_overhead_is_measured() {
    let Some(dir) = index_dir() else {
        return;
    };
    let embedder = FixedText(query_vector());
    let args = ["northwind ledger reconciliation".to_string()];
    let query = parse_query(&args).expect("the query parses");

    // Warm-up: plans, caches, and the first `Store::open` inside each arm.
    let _ = search(&dir, &query, None, Some(&embedder), 10).expect("a warm-up search");

    const REPS: usize = 20;
    let mut fused = Vec::with_capacity(REPS);
    let mut text_arm = Vec::with_capacity(REPS);
    for _ in 0..REPS {
        let started = Instant::now();
        let outcome = search(&dir, &query, None, Some(&embedder), 10).expect("the fused search");
        fused.push(ms(started));
        let timing = outcome
            .timings
            .iter()
            .find(|timing| timing.arm.name() == "meaning")
            .expect("the meaning arm ran");
        text_arm.push(timing.millis);
        assert!(!outcome.hits.is_empty(), "the fused search found nothing");
    }

    println!("\n-- fused search, fixed-vector meaning arm, no image arm --");
    percentiles("fused total", fused);
    percentiles("meaning arm", text_arm);
}

/// What a fresh `Store::open` costs: every arm of a fused query opens its own
/// connection, so four of these land inside the total.
#[test]
#[ignore = "measurement: needs an existing index (TESSERA_QUERY_INDEX); run in release"]
fn store_open_is_measured() {
    let Some(dir) = index_dir() else {
        return;
    };

    // Warm-up: pragma cache and the migration state read.
    drop(Store::open(&dir).expect("a warm-up open"));

    const REPS: usize = 40;
    let mut opens = Vec::with_capacity(REPS);
    for _ in 0..REPS {
        let started = Instant::now();
        let store = Store::open(&dir).expect("the index is openable");
        opens.push(ms(started));
        drop(store);
    }
    percentiles("Store::open", opens);
}

/// The passes fusion runs after the arms: evidence for every candidate, and
/// the display rows for the survivors. These sit inside the fused total but
/// outside every arm timer, so an arm-only measurement cannot see them.
#[test]
#[ignore = "measurement: needs an existing index (TESSERA_QUERY_INDEX); run in release"]
fn fusion_shared_passes_are_measured() {
    let Some(dir) = index_dir() else {
        return;
    };
    let store = Store::open(&dir).expect("the index is openable");
    let filter = Filter::default();

    // Warm-up: plans and page cache.
    let all = store.filtered_file_ids(&filter).expect("a warm-up listing");
    let _ = store
        .tile_text_lengths(&all)
        .expect("a warm-up evidence pass");
    println!("index: {} indexed file(s)", all.len());

    const REPS: usize = 12;
    let mut listing = Vec::with_capacity(REPS);
    let mut evidence = Vec::with_capacity(REPS);
    let mut displays = Vec::with_capacity(REPS);
    for _ in 0..REPS {
        let started = Instant::now();
        let ids = store.filtered_file_ids(&filter).expect("the listing");
        listing.push(ms(started));

        let started = Instant::now();
        let lengths = store.tile_text_lengths(&ids).expect("the evidence pass");
        evidence.push(ms(started));
        assert!(!lengths.is_empty(), "the evidence pass found no tiles");

        let started = Instant::now();
        let rows = store
            .file_displays(&ids[..ids.len().min(10)])
            .expect("the display pass");
        displays.push(ms(started));
        assert!(!rows.is_empty(), "the display pass found no rows");
    }

    println!("\n-- fusion shared passes, no arms, no model --");
    percentiles("filtered_file_ids", listing);
    percentiles("tile_text_lengths", evidence);
    percentiles("file_displays(10)", displays);
}
