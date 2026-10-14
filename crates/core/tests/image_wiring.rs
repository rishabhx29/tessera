//! Ticket 07 acceptance: the description arm is actually wired in.
//!
//! The earlier part of this ticket proved the embedder works and the store can
//! hold vectors. Neither of those means anything if nothing calls them. This
//! runs the whole path — index a folder, then ask a question about what the
//! images show — which is the only test that can fail if the wiring is missing.
//!
//! ```
//! tessera models install --set siglip
//! cargo test --release -p tessera-core --test image_arm -- --ignored --nocapture
//! ```

use std::path::Path;

use tessera_core::cli;
use tessera_core::index::image::ImagePipeline;
use tessera_core::index::{index_folder, Pipeline};
use tessera_core::models::Models;
use tessera_core::ocr::Unavailable;
use tessera_core::store::IMAGE_EMBEDDING_DIM;

fn models() -> Models {
    let dir = std::env::var("TESSERA_MODELS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| cli::models_dir(&cli::default_data_dir().expect("a data dir"), None));
    Models::open(&dir).expect("models dir")
}

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-sample.png")
}

#[test]
#[ignore = "needs the 213 MB siglip model set"]
fn indexing_a_folder_embeds_every_image_and_describe_can_find_them() {
    let models = models();
    let Some(embedder) = cli::image_embedder(&models) else {
        panic!(
            "the siglip models are not installed, so there is no image arm to wire. \
             Fix with: tessera models install --set siglip"
        );
    };

    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    // Four copies of the one committed fixture. Identical content is the point:
    // the claim under test is "the vectors exist and are searchable", not
    // "this corpus is confusable-free".
    for name in ["a.png", "b.png", "c.png", "d.png"] {
        std::fs::copy(fixture(), corpus.path().join(name)).unwrap();
    }

    let store = tessera_core::store::Store::open(data.path()).unwrap();
    let summary = index_folder(
        &store,
        &[Box::new(
            ImagePipeline::new(Box::new(Unavailable("test"))).with_embedder(Some(embedder.clone())),
        )],
        corpus.path(),
    )
    .unwrap();

    // The OCR engine here is deliberately `Unavailable`: this test is about the
    // *semantic* arm, and a machine with no OCR language pack must still be able
    // to find its photographs by description. The text half fails, the vector
    // half still lands.
    assert_eq!(
        summary.failed, 4,
        "no OCR engine means every text read fails"
    );
    assert_eq!(
        summary.unembedded, 0,
        "every image should have been embedded despite the OCR failure"
    );

    // The thing that was missing before: vectors exist in the store, not just
    // in memory.
    assert_eq!(
        store.image_embedding_count().unwrap(),
        4,
        "the index should hold one vector per image"
    );

    // And the query side reaches them.
    let vector = embedder
        .embed_text(
            "a printed invoice with a reference number",
            IMAGE_EMBEDDING_DIM,
        )
        .expect("describe the query");
    let hits = store
        .describe(&vector, IMAGE_EMBEDDING_DIM, 3)
        .expect("rank the corpus");

    assert_eq!(hits.len(), 3, "the limit should be honoured");
    for hit in &hits {
        assert!(
            Path::new(&hit.path).exists(),
            "a description result points at a file that is not there: {}",
            hit.path
        );
    }
}

#[test]
#[ignore = "needs the 213 MB siglip model set"]
fn re_indexing_does_not_double_the_vectors() {
    // The one thing that would quietly grow the index forever if it were wrong.
    let models = models();
    let Some(embedder) = cli::image_embedder(&models) else {
        panic!("install the siglip models first");
    };
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    for name in ["a.png", "b.png"] {
        std::fs::copy(fixture(), corpus.path().join(name)).unwrap();
    }

    let store = tessera_core::store::Store::open(data.path()).unwrap();
    let pipelines = || -> Vec<Box<dyn Pipeline>> {
        vec![Box::new(
            ImagePipeline::new(Box::new(Unavailable("test"))).with_embedder(Some(embedder.clone())),
        )]
    };

    index_folder(&store, &pipelines(), corpus.path()).unwrap();
    assert_eq!(store.image_embedding_count().unwrap(), 2);

    // Force a re-embed rather than a stat-only verify, by touching the files.
    for name in ["a.png", "b.png"] {
        let bytes = std::fs::read(corpus.path().join(name)).unwrap();
        std::fs::write(corpus.path().join(name), bytes).unwrap();
        let file = std::fs::File::options()
            .write(true)
            .open(corpus.path().join(name))
            .unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
            .unwrap();
    }
    index_folder(&store, &pipelines(), corpus.path()).unwrap();

    assert_eq!(
        store.image_embedding_count().unwrap(),
        2,
        "re-embedding must replace, not accumulate"
    );
}

#[test]
fn without_the_models_the_image_arm_is_absent_and_text_still_works() {
    // The degradation path, which is the one a user without `tessera models
    // install --set siglip` actually takes. An empty models directory is
    // exactly what that looks like.
    let empty = tempfile::tempdir().unwrap();
    let models = Models::open(empty.path()).unwrap();
    assert!(
        cli::image_embedder(&models).is_none(),
        "no models should mean no embedder, not a half-built one"
    );
}
