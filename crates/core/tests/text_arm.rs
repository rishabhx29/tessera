//! Ticket 09 acceptance: the text arm, on the real pinned MiniLM model.
//!
//! The arm's bookkeeping (the sweep, the pin, the tile → file collapse) is
//! tested without ONNX in the unit tests. This file is the other half: the
//! model actually puts a lease next to a query that shares no words with it.
//!
//! These tests need the pinned `minilm` weights, which CI does not have and
//! which no test suite should download on its own. So each one asks whether the
//! models are installed, and says out loud that it is skipping rather than
//! passing quietly. A green run that proved nothing is the failure mode this
//! file exists to avoid.
//!
//! Run them with:
//!
//! ```text
//! tessera models install --set minilm
//! cargo test -p tessera-core --test text_arm
//! ```

use std::path::PathBuf;
use std::time::Instant;

use tessera_core::index::{index_folder_with_text, text::TextPipeline};
use tessera_core::models::Models;
use tessera_core::store::{Store, TEXT_EMBEDDING_DIM};
use tessera_core::vectors::minilm::MiniLm;

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

/// The embedder, or `None` with an explanation. Skipping is stated, never silent.
fn embedder() -> Option<MiniLm> {
    let models = models();
    if !MiniLm::is_installed(&models) {
        eprintln!(
            "skipping: the minilm models are not installed. Run: tessera models install --set minilm"
        );
        return None;
    }
    match MiniLm::new(&models, 4) {
        Ok(embedder) => Some(embedder),
        Err(err) => panic!("the models are installed but the embedder refused to load: {err:#}"),
    }
}

fn write(dir: &std::path::Path, name: &str, contents: &str) {
    std::fs::write(dir.join(name), contents).unwrap();
}

#[test]
fn a_meaning_query_finds_a_document_that_shares_none_of_its_words() {
    let Some(embedder) = embedder() else {
        return;
    };
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let lease = "The tenant agrees that for two years after leaving the company, \
                 they will not accept employment with any competing business \
                 within fifty miles of the office.";
    write(corpus.path(), "lease.txt", lease);
    write(
        corpus.path(),
        "invoice.txt",
        "British Airways flight 118, seat 23A, Heathrow to Boston, paid in full.",
    );
    write(
        corpus.path(),
        "recipe.txt",
        "Whisk the egg whites until stiff, then fold in the almonds and bake \
         for twenty minutes.",
    );

    let store = Store::open(data.path()).unwrap();
    let summary =
        index_folder_with_text(&store, &[Box::new(TextPipeline)], corpus.path(), &embedder)
            .unwrap();
    assert_eq!(summary.indexed, 3);
    assert_eq!(summary.text_unembedded, 0);

    let query = "the contract that stops me working for a competitor";
    let vector = embedder.embed_query(query).unwrap();
    let hits = store.describe_text(&vector, TEXT_EMBEDDING_DIM, 3).unwrap();
    assert!(!hits.is_empty(), "the arm returned nothing at all");
    assert!(
        hits[0].path.ends_with("lease.txt"),
        "expected the lease, got {:?}",
        hits.iter().map(|hit| &hit.path).collect::<Vec<_>>()
    );

    // The point of the arm: the lease never says "competitor", and the query
    // never says "tenant" or "employment". The match is in the embedding, not
    // in the words.
    assert!(
        !lease.to_lowercase().contains("competitor"),
        "the fixture must not contain the query's word, or this proves nothing"
    );

    // The file rows record the model that built their vectors, so a pin move
    // is detectable rather than a silent blend.
    assert_eq!(
        store.arm_version(tessera_core::store::Arm::Text).unwrap(),
        Some(embedder.version().to_string())
    );
}

#[test]
#[ignore = "measurement: needs the minilm models; run in release with TESSERA_MODELS_DIR set"]
fn query_embedding_latency_is_measured() {
    let Some(embedder) = embedder() else {
        return;
    };

    let queries = [
        "a contract that stops me working for a competitor",
        "the invoice from the airline",
        "a receipt for a hotel stay",
        "notes from the meeting about the budget",
        "a photograph of a dog on a beach",
        "the lease clause about the roof",
        "flight confirmation for Heathrow",
        "the recipe with almonds",
    ];

    // One warm-up pass: the first run includes session warm-up and would be
    // reported as the model's cost when it is the process's.
    embedder.embed_query("warm up the session").unwrap();

    let mut samples = Vec::new();
    for i in 0..40 {
        let started = Instant::now();
        let vector = embedder.embed_query(queries[i % queries.len()]).unwrap();
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(vector.dim(), TEXT_EMBEDDING_DIM);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| samples[((samples.len() - 1) as f64 * p).round() as usize];
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    println!(
        "minilm query embedding: p50 {:.1} ms, p95 {:.1} ms, mean {:.1} ms, n {}",
        at(0.5),
        at(0.95),
        mean,
        samples.len()
    );
}
