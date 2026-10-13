//! OCR: turning images into the text a person can search for.
//!
//! The seam is [`OcrEngine`]. The image pipeline depends on the trait and never
//! on a platform API, which is what makes the Windows, macOS and ONNX engines
//! interchangeable additions rather than a rewrite (ADR-0007).
//!
//! The engine is handed a **path, not pixels**, and that is the whole design.
//!
//! - Windows decodes an image in a few milliseconds, understands every format
//!   the OS renders, and runs the recognition natively: measured **p50 43.8 ms**
//!   over this machine's 40-screenshot corpus (`docs/performance.md`), against
//!   ~2.4 s for the ONNX fallback on the same corpus. Decoding it ourselves
//!   first would add work and lose formats for nothing.
//! - A pure-library engine (the ONNX fallback, ticket 05) decodes with whatever
//!   crate it likes and takes exactly as long as it takes.
//!
//! So who decodes is the engine's business, and the seam says only *what* must
//! come out: the text visible in the file.
//!
//! When v0.2 needs to read a video frame that exists only in memory, this
//! interface gains a pixels variant *then* — with a caller who needs it, rather
//! than a speculative one now.

#[cfg(windows)]
pub mod windows;

use std::path::Path;

use anyhow::Result;

/// Something that reads the text out of an image file.
///
/// Small on purpose. An engine that cannot read the file returns an error like
/// any other failure, and the caller cannot tell whether it came from the
/// format, from the file, or from the engine — because at this seam it should
/// not be able to tell, and the pipeline's job is to record the failure rather
/// than diagnose it.
pub trait OcrEngine: Send + Sync {
    /// A stable name for diagnostics: which engine produced a tile's text.
    fn name(&self) -> &'static str;

    /// The text visible in the image, or an error if it could not be read.
    fn recognize(&self, path: &Path) -> Result<String>;
}

/// The engine to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrChoice {
    /// The best available engine for this machine.
    Native,
    /// A named engine. Fails loudly when unavailable rather than silently
    /// falling back: an engine the user chose and did not get is not a
    /// preference, it is a bug.
    Named(String),
}

/// A resolved engine and the name it will be recorded under.
pub struct EngineSelection {
    pub engine: Box<dyn OcrEngine>,
    pub name: String,
}

impl std::fmt::Debug for EngineSelection {
    /// Names the engine, never dumps it: a `Box<dyn OcrEngine>` is not
    /// printable, and the name is the part a diagnostic ever needs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineSelection")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl EngineSelection {
    /// Build from any engine, taking its name from the engine itself — the one
    /// place that knows it.
    pub fn of(engine: Box<dyn OcrEngine>) -> Self {
        Self {
            name: engine.name().to_string(),
            engine,
        }
    }
}

/// Resolve an [`OcrChoice`] into an engine.
pub fn select(choice: &OcrChoice) -> Result<EngineSelection> {
    match choice {
        OcrChoice::Native => Ok(EngineSelection::of(default_engine())),
        OcrChoice::Named(name) => Ok(EngineSelection::of(named_engine(name)?)),
    }
}

/// The name of the engine this machine uses by default, even when no engine is
/// available — so diagnostics can say "none" rather than failing to name it.
pub fn default_engine_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else {
        "onnx"
    }
}

/// The best available engine on this machine, or an engine that fails every
/// request with an explanation.
///
/// Not a hard error: the image pipeline records a failed file either way, and a
/// machine with no OCR language pack installed should get one clear message per
/// file rather than a refusal to start indexing at all.
pub fn default_engine() -> Box<dyn OcrEngine> {
    #[cfg(windows)]
    if let Ok(engine) = windows::WindowsOcr::new() {
        return Box::new(engine);
    }
    Box::new(Unavailable(default_engine_name()))
}

/// Whether this machine can recognise text at all. Cheap enough to ask before a
/// long run, so "every screenshot came back empty" is explained up front instead
/// of after an hour of indexing.
pub fn is_available() -> bool {
    #[cfg(windows)]
    {
        windows::WindowsOcr::new().is_ok()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// How to get OCR working on Windows, said once so the advice cannot drift
/// between the adapter that reports a missing language pack and the engine that
/// stands in for it.
pub const WINDOWS_LANGUAGE_PACK_HELP: &str = "install an OCR language pack \
    (Settings > Time & language > Language & region > Language options > \
    Optical character recognition)";

/// An engine that is not there. Fails every request, loudly and usefully,
/// rather than returning empty text: an image pipeline that gets empty text for
/// every screenshot looks like a working index that cannot find anything, which
/// is the worst failure there is to debug.
pub struct Unavailable(pub &'static str);

impl OcrEngine for Unavailable {
    fn name(&self) -> &'static str {
        self.0
    }

    fn recognize(&self, _path: &Path) -> Result<String> {
        anyhow::bail!(
            "the {} OCR engine is not available on this machine. On Windows: {}. \
             The cross-platform ONNX engine lands with ticket 05.",
            self.0,
            WINDOWS_LANGUAGE_PACK_HELP
        )
    }
}

fn named_engine(name: &str) -> Result<Box<dyn OcrEngine>> {
    match name {
        #[cfg(windows)]
        "windows" => Ok(Box::new(windows::WindowsOcr::new()?)),
        #[cfg(not(windows))]
        "windows" => anyhow::bail!("the Windows OCR engine does not exist on this platform"),
        "onnx" => anyhow::bail!(
            "the ONNX OCR engine lands with ticket 05; {} is not available yet",
            default_engine_name()
        ),
        other => anyhow::bail!("no OCR engine named {other:?} (windows, onnx)"),
    }
}

/// OCR engines return text in reading order, line by line. Keeping the lines
/// whole is worth more to a search index than any tidying: a query spanning two
/// lines should match across them, which joining into one string would destroy.
pub fn lines_into_text(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_kept_in_reading_order_and_blanks_dropped() {
        assert_eq!(
            lines_into_text(&["first".into(), "  ".into(), "second".into()]),
            "first\nsecond"
        );
        assert_eq!(lines_into_text(&[]), "");
    }

    #[test]
    fn an_absent_engine_fails_loudly_rather_than_returning_empty_text() {
        let engine = Unavailable("onnx");
        let err = engine.recognize(Path::new("anything.png")).unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains("not available"));
        assert!(
            message.contains("language pack") || message.contains("ticket 05"),
            "the message must tell the user what to do: {message}"
        );
    }

    #[test]
    fn an_unknown_engine_name_is_named_in_the_error() {
        let Err(err) = named_engine("tesseract") else {
            panic!("an engine we do not have must not resolve");
        };
        assert!(format!("{err:#}").contains("tesseract"));
    }

    #[test]
    fn the_native_choice_always_resolves_to_something_that_names_itself() {
        // On a machine without an engine this resolves to Unavailable, which is
        // deliberate: selection must not fail, because the pipeline's job is to
        // record failures per file rather than refuse to start.
        let selected = select(&OcrChoice::Native).unwrap();
        assert_eq!(selected.name, selected.engine.name());
        assert_eq!(selected.name, default_engine_name());
    }

    #[test]
    fn a_named_choice_keeps_the_name_it_was_given() {
        #[cfg(windows)]
        {
            let selected = select(&OcrChoice::Named("windows".into())).unwrap();
            assert_eq!(selected.name, "windows");
            assert_eq!(selected.name, selected.engine.name());
        }
        // An engine that does not exist here is an error, not a silent
        // substitution for something else.
        let Err(err) = select(&OcrChoice::Named("nope".into())) else {
            panic!("an engine we do not have must not resolve");
        };
        assert!(format!("{err:#}").contains("nope"));
    }
}
