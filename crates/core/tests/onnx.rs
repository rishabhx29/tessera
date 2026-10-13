//! Ticket 05 acceptance: the cross-platform engine, on real models.
//!
//! These tests need the pinned `ppocr` weights, which CI does not have and which
//! no test suite should download on its own. So each one asks whether the models
//! are installed, and says out loud that it is skipping rather than passing
//! quietly. A green run that proved nothing is the failure mode this file exists
//! to avoid.
//!
//! Run them with:
//!
//! ```text
//! tessera models install --set ppocr
//! cargo test -p tessera-core --test onnx
//! ```

use std::path::{Path, PathBuf};

use tessera_core::index::{image::ImagePipeline, index_folder, Pipeline};
use tessera_core::models::Models;
use tessera_core::ocr::onnx::{self, OnnxOcr};
use tessera_core::ocr::{OcrChoice, OcrEngine};
use tessera_core::store::Store;

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

/// The engine, or `None` with an explanation. Skipping is stated, never silent.
fn engine() -> Option<OnnxOcr> {
    let models = models();
    if !OnnxOcr::is_installed(&models) {
        eprintln!(
            "skipping: the ppocr models are not installed. Run: tessera models install --set ppocr"
        );
        return None;
    }
    match OnnxOcr::new(&models) {
        Ok(engine) => Some(engine),
        Err(err) => panic!("the models are installed but the engine refused to load: {err:#}"),
    }
}

/// The committed synthetic screenshot: rendered words, no private content
/// (SECURITY.md). Words chosen to be unmistakable to an OCR engine.
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-sample.png")
}

#[test]
fn the_engine_reads_the_text_of_a_screenshot() {
    let Some(engine) = engine() else {
        return;
    };
    let lines = engine.lines(&fixture()).expect("the fixture is readable");

    assert!(!lines.is_empty(), "the engine found no text at all");
    let text = lines
        .iter()
        .map(|(text, _)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();

    // The words are on the image. The recogniser is not perfect — on this fixture
    // it reads "INV01CE" for "INVOICE" — so these are matched loosely on purpose:
    // the claim being tested is that text is *found and read*, not that the model
    // is character-perfect. Quality is measured on the real corpus instead.
    assert!(text.contains("tessera"), "{text}");
    assert!(text.contains("3391"), "{text}");
    assert!(
        text.contains("invoice") || text.contains("inv01ce"),
        "expected the invoice heading, loosely: {text}"
    );
    assert!(text.contains("search engine"), "{text}");
}

#[test]
fn every_line_it_keeps_clears_the_confidence_gate() {
    let Some(engine) = engine() else {
        return;
    };
    for (text, score) in engine.lines(&fixture()).expect("the fixture is readable") {
        assert!(
            score >= onnx::SCORE_GATE,
            "{text:?} was kept at {score}, below the gate"
        );
        assert!(
            text.chars().count() > 1,
            "{text:?} was kept as a single character"
        );
    }
}

#[test]
fn the_recogniser_looks_like_it_emits_probabilities_not_logits() {
    // ADR-0008's trap, held against the real file: if a pin ever moved to a
    // logits export, the engine's constructor would refuse (its alphabet is
    // checked against the graph's class count), and failing that, a real line
    // would not clear the gate — because applying softmax to probabilities
    // crushes 0.94 into 0.0001. Asserted on a real line's score, not on the
    // internal representation.
    let Some(engine) = engine() else {
        return;
    };
    let kept = engine.lines(&fixture()).expect("the fixture is readable");
    assert!(
        kept.iter().any(|(_, score)| *score > 0.9),
        "at least one line should read with high confidence; got {kept:?}"
    );
}

#[test]
fn text_inside_an_image_is_searchable_through_the_cross_platform_engine() {
    let Some(engine) = engine() else {
        return;
    };
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    let screenshot = dir.join("screenshot-2026.png");
    std::fs::copy(fixture(), &screenshot).unwrap();

    let store = Store::open(data.path()).unwrap();
    let pipeline: Box<dyn Pipeline> = Box::new(ImagePipeline::new(Box::new(engine)));
    let summary = index_folder(&store, &[pipeline], dir).unwrap();
    assert_eq!(
        (summary.indexed, summary.failed),
        (1, 0),
        "the screenshot indexed"
    );

    // A word that exists only inside the image.
    let hits = tessera_core::search::search(&store, &["3391"]).unwrap();
    assert_eq!(
        hits.iter().map(|h| h.path.clone()).collect::<Vec<_>>(),
        vec![screenshot.to_string_lossy().to_string()],
        "text inside an image should be findable through this engine too"
    );
}

#[test]
fn a_file_that_is_not_an_image_is_an_error_naming_it() {
    let Some(engine) = engine() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("broken.png");
    std::fs::write(&broken, b"\x89PNG\r\n\x1a\n not a real image at all").unwrap();

    let err = engine.recognize(&broken).unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("broken.png"), "{message}");
    assert!(
        !engine.recognize(&dir.path().join("absent.png")).is_ok(),
        "a missing file must be an error, not empty text"
    );
}

#[test]
fn asking_for_it_by_name_resolves_to_this_engine() {
    if !OnnxOcr::is_installed(&models()) {
        eprintln!("skipping: the ppocr models are not installed");
        return;
    }
    let selection = tessera_core::ocr::select(&OcrChoice::Named("onnx".into()), &models())
        .expect("a named engine that is installed resolves");
    assert_eq!(selection.name, "onnx");
    assert_eq!(selection.name, selection.engine.name());
}

#[test]
fn two_engines_can_exist_in_one_process() {
    // ONNX Runtime allows exactly one *environment* per process. An engine per
    // constructor therefore used to work once and then fail with "only one
    // environment is allowed per process" — which is what the CLI would have hit
    // the moment it built a second engine, and what this test file hit the moment
    // its tests ran in parallel. The environment is created once and shared, so
    // building another engine is just more sessions.
    let Some(_) = engine() else {
        return;
    };
    let models = models();
    OnnxOcr::new(&models).expect("a second engine in the same process must be buildable");
    // And a third, for the case that is one step further from the obvious fix.
    OnnxOcr::new(&models).expect("and a third");
}

#[test]
fn the_engine_is_shareable_across_threads() {
    // `OcrEngine: Send + Sync` and ONNX Runtime sessions are not safe for
    // concurrent `run`. If this compiles and passes, the session pool is doing
    // what it exists to do.
    let Some(engine) = engine() else {
        return;
    };
    let shared = std::sync::Arc::new(engine);
    let fixture = fixture();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let engine = std::sync::Arc::clone(&shared);
            let path = fixture.clone();
            std::thread::spawn(move || engine.recognize(&path).is_ok())
        })
        .collect();
    for handle in handles {
        assert!(
            handle.join().unwrap(),
            "every thread should get a readable image"
        );
    }
}
