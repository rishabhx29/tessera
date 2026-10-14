//! Ticket 04 acceptance: text inside images becomes searchable, through the
//! OCR seam and no platform API anywhere else.
//!
//! Two engines are exercised. The **real** Windows adapter reads a committed
//! fixture with known words, because a seam tested only with a fake is a seam
//! that ships broken. The **fake** engine covers what the real one cannot
//! prove deterministically: that a decode failure is recorded against one file
//! while the run continues.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use tessera_core::index::image::ImagePipeline;
use tessera_core::index::{index_folder, IndexSummary, Kind, Pipeline, Tile};
use tessera_core::ocr::{OcrChoice, OcrEngine};
use tessera_core::search::search;
use tessera_core::store::Store;

/// A committed synthetic screenshot: rendered text, no private content in the
/// repository (SECURITY.md). Words chosen to be unmistakable to an OCR engine
/// and to the typo-tolerant search.
fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-sample.png")
}

/// The models directory, taken from `--models-dir` or the default beside the
/// index. Engine choice depends on whether the OCR weights are installed, so a
/// test that says nothing about that must follow whatever the environment says
/// rather than assume an empty one.
fn models() -> tessera_core::models::Models {
    let dir = std::env::var("TESSERA_MODELS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            tessera_core::cli::models_dir(
                &std::env::temp_dir().join(format!("tessera-images-{}", std::process::id())),
                None,
            )
        });
    tessera_core::models::Models::open(&dir).expect("the models dir is openable")
}

fn copy_fixture(dir: &Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::copy(fixture(), &path).expect("the fixture is committed");
    path
}

fn hits(store: &Store, term: &str) -> Vec<String> {
    search(store, &[term])
        .unwrap()
        .into_iter()
        .map(|hit| hit.path)
        .collect()
}

fn only_hit(store: &Store, term: &str) -> String {
    let found = hits(store, term);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one hit for {term:?}, got {found:?}"
    );
    found.into_iter().next().unwrap()
}

/// An engine that fails on files whose names contain `marker`, standing in for
/// a decode failure. Everything else reads a fixed string.
struct Selective {
    marker: &'static str,
    reads: AtomicUsize,
}

impl OcrEngine for Selective {
    fn name(&self) -> &'static str {
        "selective"
    }

    fn recognize(&self, path: &Path) -> Result<String> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if path.to_string_lossy().contains(self.marker) {
            anyhow::bail!("decoding {}: not a readable image", path.display());
        }
        Ok(format!("the invoice from {}", path.display()))
    }
}

fn image_pipeline(engine: Box<dyn OcrEngine>) -> Box<dyn Pipeline> {
    Box::new(ImagePipeline::new(engine))
}

#[test]
fn text_inside_an_image_is_searchable() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    let screenshot = copy_fixture(dir, "invoice-qf-3391.png");
    write(dir, "notes.txt", "a plain text file about zebras");

    let store = Store::open(data.path()).unwrap();
    let summary = index_folder(
        &store,
        &[
            Box::new(tessera_core::index::text::TextPipeline),
            image_pipeline(Box::new(FakeReading)),
        ],
        dir,
    )
    .unwrap();
    assert_eq!(summary.indexed, 2, "both files index");

    // The word is visible only inside the image.
    let found = hits(&store, "3391");
    assert_eq!(found.len(), 1, "text inside an image should be findable");
    assert_eq!(found[0], screenshot.to_string_lossy());

    // Typo tolerance applies to OCR output like any other text.
    assert_eq!(only_hit(&store, "inv0ice"), screenshot.to_string_lossy());
    assert_eq!(only_hit(&store, "Tessera"), screenshot.to_string_lossy());
    // A query spanning two words of one line still matches.
    assert_eq!(only_hit(&store, "invoice qf"), screenshot.to_string_lossy());
    // And the text file is still searchable, so the two formats coexist.
    assert_eq!(hits(&store, "zebras").len(), 1);
}

/// An engine that returns the fixture's words, so the end-to-end path can be
/// asserted without depending on which machines have an OCR language pack.
struct FakeReading;

impl OcrEngine for FakeReading {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn recognize(&self, _path: &Path) -> Result<String> {
        Ok("INVOICE QF-3391\nTessera search engine\nTotal due: 42.50 EUR".to_string())
    }
}

fn write(dir: &Path, name: &str, contents: &str) {
    fs::write(dir.join(name), contents).unwrap();
}

#[test]
fn the_platform_engine_reads_the_text_of_a_screenshot() {
    // Renamed from `the_real_windows_engine_...` when ticket 06 landed, because
    // "the Windows engine" stopped being the only native engine and the name
    // was going to lie on a Mac. What it asserts is unchanged and is the thing
    // that matters: whichever engine this platform ships, it reads the words.
    let Ok(selection) = tessera_core::ocr::select(&OcrChoice::Native, &models()) else {
        panic!("selecting the native engine must not fail");
    };
    if !tessera_core::ocr::is_available(&models()) {
        eprintln!("skipping: no OCR engine available on this machine");
        return;
    }
    let expected_engine = tessera_core::ocr::platform_engine_name()
        .expect("this test is only meaningful where there is a platform engine");
    assert_eq!(
        selection.name, expected_engine,
        "this platform should default to its own engine"
    );

    let text = selection
        .engine
        .recognize(&fixture())
        .expect("the fixture is readable by the platform engine")
        .to_lowercase();
    for expected in ["invoice", "tessera", "3391"] {
        assert!(
            text.contains(expected),
            "the platform engine should have read {expected:?}; it read {text:?}"
        );
    }
}

/// The acceptance criterion itself, through the real engine: index a folder of
/// screenshots and find one by a word visible only inside it.
///
/// The fake-engine version of that test passes with `ocr/windows.rs` deleted,
/// which is evidence of nothing. This one runs the real path, and says so when
/// the machine cannot.
#[test]
fn text_inside_a_screenshot_is_findable_through_the_real_engine() {
    if !tessera_core::ocr::is_available(&models()) {
        eprintln!("skipping: no Windows OCR language pack on this machine");
        return;
    }
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    let screenshot = copy_fixture(dir, "screenshot-2026.png");

    let store = Store::open(data.path()).unwrap();
    let engine = tessera_core::ocr::select(&OcrChoice::Native, &models()).unwrap();
    let summary = index_folder(&store, &[image_pipeline(engine.engine)], dir).unwrap();
    assert_eq!(
        (summary.indexed, summary.failed),
        (1, 0),
        "the screenshot indexed"
    );

    // Words that exist nowhere but inside the image.
    assert_eq!(only_hit(&store, "3391"), screenshot.to_string_lossy());
    assert_eq!(only_hit(&store, "Tessera"), screenshot.to_string_lossy());
    // Typo tolerance applies to OCR output like any other text.
    assert_eq!(only_hit(&store, "inv0ice"), screenshot.to_string_lossy());
}

/// A genuinely undecodable file, through the real engine and the real index
/// loop: recorded as failed, with a reason, and the run carries on.
#[test]
fn a_real_undecodable_image_is_recorded_and_the_run_continues() {
    if !tessera_core::ocr::is_available(&models()) {
        eprintln!("skipping: no Windows OCR language pack on this machine");
        return;
    }
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    // A real PNG signature, a garbage body: the extension claims the file, the
    // decoder rejects it.
    fs::write(
        dir.join("broken.png"),
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR not a real image at all",
    )
    .unwrap();
    copy_fixture(dir, "good.png");
    write(dir, "notes.txt", "a text file about zebras");

    let store = Store::open(data.path()).unwrap();
    let engine = tessera_core::ocr::select(&OcrChoice::Native, &models()).unwrap();
    let summary = index_folder(
        &store,
        &[
            Box::new(tessera_core::index::text::TextPipeline),
            image_pipeline(engine.engine),
        ],
        dir,
    )
    .unwrap();

    assert_eq!(
        (summary.indexed, summary.failed),
        (2, 1),
        "one bad file, two good ones"
    );
    assert_eq!(
        hits(&store, "3391").len(),
        1,
        "the good screenshot still indexed"
    );
    assert_eq!(
        hits(&store, "zebras").len(),
        1,
        "the run continued past the bad file"
    );

    // And the failure is recorded with a reason that names the file.
    let failures = store.failures().unwrap();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].0.ends_with("broken.png"), "{failures:?}");
    assert!(
        !failures[0].1.trim().is_empty(),
        "a failed file with no reason cannot be diagnosed: {failures:?}"
    );
}

#[test]
fn the_image_pipeline_never_imports_a_platform_api() {
    // The convention is checkable, so it is checked — by walking the directory,
    // not by a hand-written file list that stops covering new files.
    //
    // Crate paths, not the substring "windows": `std::os::windows::fs` is a
    // metadata extension, while `windows::Graphics` is WinRT.
    let forbidden = [
        "use windows",
        "windows::Foundation",
        "windows::Graphics",
        "windows::Storage",
        "windows::Media",
        "objc2",
        "coreml",
        "vision_framework",
    ];
    // The documented exemptions: `ocr/<platform>` *is* the adapter, and
    // `index/skip.rs` asks the filesystem what a file is — `std::os::windows`
    // is how you ask on Windows, and that has nothing to do with reading text.
    //
    // One entry per adapter, and the list is the test: adding a platform means
    // adding its exemption here, so a new adapter cannot silently widen the
    // seam without someone noticing that this constant changed.
    const EXEMPT: [&str; 3] = ["index/skip.rs", "ocr/windows.rs", "ocr/vision.rs"];

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut checked = 0;
    let mut seen_exemptions: Vec<String> = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if !path.extension().is_some_and(|ext| ext == "rs") {
                continue;
            }
            let relative = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let source = fs::read_to_string(&path).unwrap();
            checked += 1;
            if EXEMPT.contains(&relative.as_str()) {
                // An exemption for a file that no longer exists is a hole that
                // reads as coverage: the next adapter dropped in there would
                // inherit it. So the exemption has to be spent on a file that is
                // actually here.
                seen_exemptions.push(relative.to_string());
                continue;
            }
            for api in forbidden {
                assert!(
                    !source.contains(api),
                    "src/{relative} mentions {api:?}: the seam exists so no engine-facing code \
                     names a platform API"
                );
            }
        }
    }
    assert!(
        checked >= 6,
        "the scan should have covered the engine's sources, saw {checked}"
    );
    // Every exemption was spent on a file that exists. A stale entry is a hole
    // that reads as coverage, and the failure is silent — which is the whole
    // reason this test walks a directory instead of listing files by hand.
    for exempt in EXEMPT {
        assert!(
            seen_exemptions.iter().any(|seen| seen == exempt),
            "{exempt} is exempted from the seam check but no such file was found. \
             Remove the exemption, or add the adapter it was meant for."
        );
    }
}
#[test]
fn a_decode_failure_is_recorded_and_the_run_continues() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    write(dir, "a.txt", "the first file");
    fs::write(dir.join("broken.png"), b"this is not a png at all").unwrap();
    write(dir, "b.txt", "the second file");

    let engine = Selective {
        marker: "broken",
        reads: AtomicUsize::new(0),
    };
    let summary = index_folder(
        &store_at(data.path()),
        &[
            Box::new(tessera_core::index::text::TextPipeline),
            image_pipeline(Box::new(engine)),
        ],
        dir,
    )
    .unwrap();

    // One bad file must not stop the run or take the others with it.
    assert_eq!((summary.indexed, summary.failed), (2, 1));

    let store = store_at(data.path());
    assert_eq!(
        hits(&store, "first").len(),
        1,
        "the files before the bad one indexed"
    );
    assert_eq!(
        hits(&store, "second").len(),
        1,
        "the run continued past the bad file"
    );

    // And the failure is recorded, with a reason that says what went wrong.
    let stats = store.stats().unwrap();
    assert!(stats.contains(&("failed".to_string(), 1)));
    let failures = store.failures().unwrap();
    assert_eq!(failures.len(), 1, "the bad file is recorded, and only it");
    assert!(failures[0].0.ends_with("broken.png"), "{failures:?}");
    assert!(
        failures[0].1.contains("not a readable image"),
        "the reason must say what went wrong: {:?}",
        failures[0].1
    );
    assert_eq!(store_kind(&store, &dir.join("broken.png")), "image");
}

fn store_at(dir: &Path) -> Store {
    Store::open(dir).unwrap()
}

/// The `kind` recorded for one path, through the same interface the index and
/// the diagnostics use.
fn store_kind(store: &Store, path: &Path) -> String {
    store
        .file_state(path)
        .unwrap()
        .map(|state| state.kind)
        .unwrap_or_default()
}

#[test]
fn a_textless_image_is_indexed_not_failed() {
    // A photo has nothing to read. It must not land in the failed column.
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    fs::write(
        dir.join("dog.png"),
        b"not really a png, but the engine read it",
    )
    .unwrap();

    struct NoText;
    impl OcrEngine for NoText {
        fn name(&self) -> &'static str {
            "no-text"
        }
        fn recognize(&self, _path: &Path) -> Result<String> {
            Ok(String::new())
        }
    }

    let store = store_at(data.path());
    let summary = index_folder(&store, &[image_pipeline(Box::new(NoText))], dir).unwrap();
    assert_eq!((summary.indexed, summary.failed), (1, 0));
    // No text means no tile text, but the filename still makes it findable —
    // which is why the loop prepends the name to the (single, empty) tile.
    assert_eq!(
        only_hit(&store, "dog"),
        dir.join("dog.png").to_string_lossy()
    );
}

#[test]
fn one_tile_per_image_and_the_kind_is_recorded() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    copy_fixture(dir, "screenshot.png");

    let store = store_at(data.path());
    let summary: IndexSummary =
        index_folder(&store, &[image_pipeline(Box::new(FakeReading))], dir).unwrap();
    assert_eq!(summary.indexed, 1);
    assert_eq!(store_kind(&store, &dir.join("screenshot.png")), "image");

    // The pipeline's own kind claim is what goes on the row, and the pipeline
    // is the only thing that decides it.
    assert_eq!(
        ImagePipeline::new(Box::new(FakeReading)).accepts(Path::new("x.png")),
        Some(Kind::Image)
    );
    let _: Tile = Tile {
        seq: 0,
        text: String::new(),
    };
}

/// The recorded reason is the only thing the user has to go on, so it has to
/// carry the whole cause chain.
///
/// `anyhow::Error::to_string()` prints only the *outermost* context. For the
/// OCR adapters that is `decoding <path>` — it names the file and says nothing
/// about why. `{err:#}` walks the chain. This test uses an engine that fails
/// with a layered error, which is the same shape every real adapter produces.
#[test]
fn a_failure_reason_carries_the_whole_cause_chain_not_just_the_outermost_context() {
    /// Wraps its inner error in context, the way the adapters do.
    struct Chained;

    impl OcrEngine for Chained {
        fn name(&self) -> &'static str {
            "chained"
        }
        fn recognize(&self, path: &Path) -> Result<String> {
            Err(anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "the bitmap header is not a bitmap",
            ))
            .context(format!("decoding {}", path.display())))
        }
    }

    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    copy_fixture(dir, "broken.png");

    let store = store_at(data.path());
    let summary = index_folder(&store, &[image_pipeline(Box::new(Chained))], dir).unwrap();
    assert_eq!(summary.failed, 1);

    let failures = store.failures().unwrap();
    let reason = &failures[0].1;
    assert!(
        reason.contains("bitmap header is not a bitmap"),
        "the recorded reason dropped the cause, keeping only context: {reason}"
    );
    assert!(
        reason.contains("decoding"),
        "the recorded reason dropped the outer context: {reason}"
    );
}

#[test]
fn an_unavailable_engine_is_reported_per_file_rather_than_crashing() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    copy_fixture(dir, "a.png");
    copy_fixture(dir, "b.png");

    let store = store_at(data.path());
    let summary = index_folder(
        &store,
        &[image_pipeline(Box::new(tessera_core::ocr::Unavailable(
            "onnx",
        )))],
        dir,
    )
    .unwrap();

    assert_eq!((summary.indexed, summary.failed), (0, 2));
    // Every one recorded, with a reason a person can act on rather than the
    // silence of an empty result.
    assert_eq!(store.stats().unwrap(), vec![("failed".to_string(), 2)]);
    let failures = store.failures().unwrap();
    assert_eq!(failures.len(), 2);
    for (_, reason) in &failures {
        // The message names the engine that was missing, not just "an OCR
        // engine" — when a user passed `--ocr-engine onnx` they need to know
        // which of the two routes failed.
        assert!(
            reason.contains("no onnx OCR engine is available"),
            "the reason does not name the engine that was unavailable: {reason}"
        );
        // Both routes to a working engine, because a machine without a language
        // pack is not necessarily a machine that should install one.
        assert!(reason.contains("language pack"), "{reason}");
        assert!(
            reason.contains("tessera models install"),
            "the reason must say what to do: {reason}"
        );
    }
}
