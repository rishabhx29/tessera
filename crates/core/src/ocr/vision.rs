//! The macOS OCR adapter: Apple's Vision framework, the engine that ships with
//! macOS.
//!
//! This is the second file in the engine that is allowed to name a platform
//! API, alongside `ocr/windows.rs`. Everything else talks to [`OcrEngine`],
//! which is what makes this and the ONNX fallback additions rather than a
//! rewrite (ADR-0007).
//!
//! Two decisions carry over from the Windows adapter, and they are the same
//! decisions rather than coincidences:
//!
//! - **The OS decodes the image.** `VNImageRequestHandler` takes a `file://`
//!   URL and Vision reads the file through ImageIO, which understands every
//!   format macOS renders. Handing it pixels would mean decoding first with a
//!   crate, then copying the result into a `CGImage`, and would be more work
//!   for fewer formats.
//! - **The adapter canonicalises its own path.** Vision resolves a relative or
//!   non-URL-escaped path to nothing and reports it as a decode failure, which
//!   says nothing about the real problem. The Windows adapter learned this the
//!   hard way; both fix it the same way.
//!
//! Unlike the Windows engine, Vision is **not** free of configuration and needs
//! **no** setup: it ships with the OS and recognises whatever the user types.
//! So `new()` cannot fail for want of a language pack.

use std::path::Path;

use anyhow::{Context as _, Result};
// `AnyThread` supplies `alloc()` and lives at the crate root, not under `rc`.
// It is a blanket impl for every type whose `ThreadKind` is `dyn AnyThread`,
// which is why the import looks unused until something calls `::alloc()`.
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_foundation::{NSArray, NSDictionary, NSURL};
use objc2_vision::{
    // Note the `d`: the observation type is `VNRecognizedTextObservation`. The
    // near-identical `VNRecognizeTextObservation` does not exist, and the
    // compiler's "a similar name exists in the module" is the only reason this
    // was caught. `results()` is typed as `NSArray<VNRecognizedTextObservation>`
    // on the request itself, so there is no cast anywhere below.
    VNImageOption,
    VNImageRequestHandler,
    VNRecognizeTextRequest,
    VNRecognizedTextObservation,
    VNRequest,
    VNRequestTextRecognitionLevel,
};

use super::{lines_into_text, OcrEngine};

/// Apple's text recogniser.
pub struct VisionOcr;

impl VisionOcr {
    /// The recogniser.
    ///
    /// Vision ships with the OS and recognises whatever the user types, so
    /// unlike the Windows engine there is no language pack to be missing and
    /// nothing to configure. The settings live in [`configured_request`], next
    /// to the request they configure, because a `new()` that built a request
    /// and threw it away would misdescribe where the behaviour lives.
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

/// A `VNRecognizeTextRequest` set up for a screenshot corpus.
///
/// The settings are not tuning knobs chosen by feel:
///
/// - `Fast` rather than `Accurate`, because indexing runs in the background
///   and the alternative is minutes of battery for a marginal accuracy gain on
///   text already rendered crisply at 1x. **This is a hypothesis, not a
///   measurement**: no macOS numbers exist yet (`docs/performance.md`), so it
///   is recorded as something to check against `Accurate` on a real Mac before
///   anyone claims anything about it.
/// - `usesLanguageCorrection` on, because a word-level correction pass is what
///   lets Vision fix the small confusions a screenshot invites.
fn configured_request() -> Retained<VNRecognizeTextRequest> {
    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Fast);
    request.setUsesLanguageCorrection(true);
    request
}

/// Read the recognised lines out of a finished request, keeping Vision's own
/// confidence for each.
///
/// Split from the FFI call so the distinction that matters is visible in one
/// place: `None` from `results()` is a **failed request**, not an image with no
/// text.
fn lines_from(request: &VNRecognizeTextRequest) -> Result<Vec<(String, Option<f32>)>> {
    // Apple's documentation for `VNRequest::results`: "If the request failed,
    // this property will be nil; otherwise, it will be an array of zero or more
    // VNObservation subclasses." So nil means the request did not run to
    // completion — a corrupt file, or a format ImageIO cannot read. Treating it
    // as "no text" would index a broken screenshot as a blank one, which is
    // the failure this engine must not have: an index that looks built and
    // finds nothing.
    let observations = request.results().context(
        "Vision ran but reported no observations at all. The file is most likely not an \
         image, or not in a format ImageIO can read.",
    )?;

    let mut lines = Vec::new();
    for observation in observations {
        // `VNRecognizeTextRequest::results` is already typed as
        // `VNRecognizedTextObservation`, so there is no cast to make and no
        // other observation type to filter out here.
        let _typed: &VNRecognizedTextObservation = &observation;

        // One candidate per observation. Vision returns up to ten alternatives
        // for the same region; a search index wants the best reading of a line,
        // not ten ways of spelling it.
        let candidates = observation.topCandidates(1);
        let Some(candidate) = candidates.firstObject() else {
            continue;
        };
        lines.push((candidate.string().to_string(), Some(candidate.confidence())));
    }
    Ok(lines)
}

impl OcrEngine for VisionOcr {
    fn name(&self) -> &'static str {
        "macos"
    }

    fn recognize(&self, path: &Path) -> Result<String> {
        let lines: Vec<String> = self
            .lines_with_scores(path)?
            .into_iter()
            .map(|(line, _)| line)
            .collect();
        // An image with no text is indexed, not failed — see the image
        // pipeline. `lines_into_text` keeps the lines whole and drops the blank
        // ones, which is the same treatment the Windows adapter gives them.
        Ok(lines_into_text(&lines))
    }

    fn lines_with_scores(&self, path: &Path) -> Result<Vec<(String, Option<f32>)>> {
        // Vision wants an absolute, percent-encoded `file://` URL. A raw path
        // resolves to nothing when it contains a space, and the failure arrives
        // as "cannot decode the image" — true, and useless. `from_file_path`
        // does the encoding and takes a `&Path`, so no lossy `to_string_lossy`
        // round-trip through a string that Foundation then re-parses.
        let path =
            dunce::canonicalize(path).with_context(|| format!("opening {}", path.display()))?;
        let url = NSURL::from_file_path(&path).with_context(|| {
            format!(
                "{} is not a path that can be expressed as a file URL (an interior NUL?)",
                path.display()
            )
        })?;

        // SAFETY: `initWithURL_options` is `unsafe` because the `options`
        // dictionary is a generic parameter objc2 cannot check — it may hold
        // keys this handler does not expect. We pass an empty dictionary, so
        // there is nothing in it to be wrong. `alloc()` is the receiver the
        // signature asks for.
        let handler = unsafe {
            VNImageRequestHandler::initWithURL_options(
                VNImageRequestHandler::alloc(),
                &url,
                &NSDictionary::<VNImageOption, AnyObject>::new(),
            )
        };

        let request = configured_request();

        // `performRequests_error` takes `NSArray<VNRequest>`, and
        // `VNRecognizeTextRequest` sits two levels below `VNRequest`
        // (`VNRecognizeTextRequest` → `VNImageBasedRequest` → `VNRequest`), so
        // `into_super()` would land on the wrong one. `AsRef<VNRequest>` is the
        // widening the generated code actually provides.
        let as_request: &VNRequest = request.as_ref();
        let requests = NSArray::from_slice(&[as_request]);
        // Only scheduling failures come back here; a per-request failure shows
        // up as nil results, which `lines_from` treats as an error.
        handler.performRequests_error(&requests).map_err(|err| {
            anyhow::anyhow!("Vision could not be given {}: {err}", path.display())
        })?;

        lines_from(&request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture every platform engine is tested against: rendered words, no
    /// private content in the repository (SECURITY.md). The same file the
    /// Windows adapter reads, so the two engines see one input.
    fn fixture() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-sample.png")
    }

    #[test]
    fn the_vision_engine_reads_the_fixture_image() {
        // The smoke test ticket 06 asks for. It asserts the real thing — that
        // Vision reads words out of a PNG — rather than that the constructor
        // returned, because an adapter that builds and then recognises nothing
        // is exactly the failure this test exists to catch.
        let engine = VisionOcr::new().expect("Vision needs no configuration");
        let text = engine
            .recognize(&fixture())
            .expect("the fixture is a readable image")
            .to_lowercase();

        assert!(
            !text.trim().is_empty(),
            "Vision read no text at all from the fixture"
        );
        // The fixture's words, case-folded: how Vision capitalises a rendered
        // word is not something to assert on.
        for expected in ["invoice", "tessera"] {
            assert!(
                text.contains(expected),
                "Vision did not read {expected:?}; it read {text:?}"
            );
        }
    }

    #[test]
    fn each_line_carries_the_confidence_vision_reported() {
        // Vision scores its output, so `lines_with_scores` must report a score
        // rather than falling back to the trait's `None`. `None` there means
        // "this engine does not score its output", which would be false for
        // Vision — the same reason the Windows engine legitimately reports
        // None there, and the reason this asymmetry is worth a test.
        let engine = VisionOcr::new().expect("Vision needs no configuration");
        let lines = engine
            .lines_with_scores(&fixture())
            .expect("the fixture is a readable image");
        assert!(!lines.is_empty(), "no lines were recognised");
        for (line, score) in &lines {
            let score = score.unwrap_or_else(|| panic!("no confidence reported for {line:?}"));
            assert!(
                (0.0..=1.0).contains(&score),
                "confidence {score} is outside Vision's documented range for {line:?}"
            );
        }
    }

    #[test]
    fn a_file_that_is_not_an_image_is_an_error_naming_it() {
        // Silence is the failure mode this engine must not have. Note this
        // asserts the *message*, not which Vision call failed: Apple documents
        // `performRequests:error:` as reporting scheduling failures only, so a
        // corrupt image may arrive either as that error or as nil results. Both
        // routes are errors here, and the test does not care which.
        let dir = tempfile::tempdir().unwrap();
        let not_an_image = dir.path().join("broken.png");
        std::fs::write(&not_an_image, b"this is not a png at all").unwrap();

        let err = match VisionOcr::new().unwrap().recognize(&not_an_image) {
            Err(err) => err,
            Ok(text) => panic!(
                "a file of plain text was indexed as an image, returning {text:?}; \
                 a broken file must be recorded as failed, not indexed blank"
            ),
        };
        let message = format!("{err:#}");
        assert!(
            message.contains("broken.png") || message.contains("no observations"),
            "the error neither names the file nor explains itself: {message}"
        );
    }
}
