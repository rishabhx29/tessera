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
//! Unlike the Windows engine, Vision is **not** free of configuration and does
//! **not** need one: it ships with the OS and recognises whatever the user
//! types. So `new()` cannot fail for want of a language pack, and the reason it
//! is still fallible is the URL construction, which is our code.

use std::path::Path;

use anyhow::{Context as _, Result};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSDictionary, NSString, NSURL};
use objc2_vision::{
    VNImageOption, VNImageRequestHandler, VNRecognizeTextObservation, VNRecognizeTextRequest,
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
    /// to the request they configure, because a `new()` that builds a request
    /// and throws it away would be a lie about where the behaviour lives.
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
/// Split out from the FFI so it can be reasoned about (and tested) as a plain
/// function over observations rather than as part of the call that produces
/// them.
fn lines_from(request: &VNRecognizeTextRequest) -> Vec<(String, Option<f32>)> {
    let Some(observations) = request.results() else {
        return Vec::new();
    };

    let mut lines = Vec::new();
    for observation in observations {
        // `downcast_ref` rather than a cast: the handler is generic, so results
        // may in principle carry other observation types, and an observation
        // that is not text is skipped rather than made to be one.
        let Some(observation) = observation.downcast_ref::<VNRecognizedTextObservation>() else {
            continue;
        };
        // One candidate per observation. Vision returns up to ten alternatives
        // for the same region; a search index wants the best reading of a
        // line, not ten ways of spelling it.
        let candidates = observation.topCandidates(1);
        let Some(candidate) = candidates.first() else {
            continue;
        };
        lines.push((candidate.string().to_string(), Some(candidate.confidence())));
    }
    lines
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
        // pipeline. `lines_into_text` keeps the lines whole and drops the
        // blank ones, which is the same treatment every engine gets.
        Ok(lines_into_text(&lines))
    }

    fn lines_with_scores(&self, path: &Path) -> Result<Vec<(String, Option<f32>)>> {
        // Vision wants an absolute, URL-escaped `file://` URL. A raw path
        // resolves to nothing on a path with a space in it, and the failure
        // arrives as "cannot decode the image" — which is true and useless.
        let path =
            dunce::canonicalize(path).with_context(|| format!("opening {}", path.display()))?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
            .with_context(|| format!("building a file URL for {}", path.display()))?;

        // No options: an empty dictionary is what Vision wants for a plain
        // image on disk, and passing the wrong generic here is the
        // `unsafe fn` footgun objc2 documents.
        let handler = unsafe {
            VNImageRequestHandler::initWithURL_options(
                VNImageRequestHandler::alloc(),
                &url,
                &NSDictionary::<VNImageOption, AnyObject>::new(),
            )
        };

        let request = configured_request();

        // SAFETY: `performRequests_error` is `unsafe` because objc2 cannot
        // prove the array holds requests it recognises. It does: this array
        // holds the one request built on the line above, and `VNRequest` is the
        // superclass `VNRecognizeTextRequest` derefs to, so the element type
        // matches what the method expects.
        let requests = NSArray::from_retained_slice(&[Retained::retain(&request).into_super()]);
        unsafe {
            handler.performRequests_error(&requests).map_err(|err| {
                anyhow::anyhow!("Vision could not read {}: {err}", path.display())
            })?;
        }

        Ok(lines_from(&request))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture every platform engine is tested against: rendered words, no
    /// private content in the repository (SECURITY.md). The same file the
    /// Windows adapter reads, so the two engines are measured on one input.
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
        // The fixture's words. Case-folded because Vision's capitalisation of a
        // rendered word is not something to assert on.
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
        // rather than the trait's `None` fallback. `None` there means "this
        // engine does not score its output", which would be false for Vision —
        // the same reason the Windows engine legitimately reports None there.
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
        // Silence is the failure mode this engine must not have: an index that
        // looks built and finds nothing.
        let dir = tempfile::tempdir().unwrap();
        let not_an_image = dir.path().join("broken.png");
        std::fs::write(&not_an_image, b"this is not a png at all").unwrap();

        let err = VisionOcr::new()
            .unwrap()
            .recognize(&not_an_image)
            .expect_err("a file of text is not an image");
        let message = format!("{err:#}");
        assert!(
            message.contains("broken.png"),
            "the error does not name the file: {message}"
        );
    }
}
