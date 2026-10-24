//! OCR: turning images into the text a person can search for.
//!
//! The seam is [`OcrEngine`]. The image pipeline depends on the trait and never
//! on a platform API, which is what makes the Windows, macOS and ONNX engines
//! interchangeable additions rather than a rewrite (ADR-0007).
//!
//! The engine is handed a **path, not pixels**, and that is the whole design.
//!
//! - Windows decodes an image in a few milliseconds, understands every format
//!   the OS renders, and runs the recognition natively: measured **p50 34.4 ms**
//!   over this machine's 40-screenshot corpus, extracting 188 characters.
//! - A pure-library engine (the ONNX fallback, now built) decodes with whatever
//!   crate it likes and takes exactly as long as it takes: measured **p50
//!   949.6 ms** on the same corpus, extracting 288 — and reading the one image
//!   the native engine found no text in at all. Both figures, with hardware and
//!   method, in `docs/performance.md`.
//!
//! So who decodes is the engine's business, and the seam says only *what* must
//! come out: the text visible in the file.
//!
//! When v0.2 needs to read a video frame that exists only in memory, this
//! interface gains a pixels variant *then* — with a caller who needs it, rather
//! than a speculative one now.

pub mod onnx;
#[cfg(target_os = "macos")]
pub mod vision;
#[cfg(windows)]
pub mod windows;

use std::path::Path;

use anyhow::Result;

use crate::models::Models;

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

    /// The image's text line by line, with each engine's own confidence where it
    /// has one.
    ///
    /// `None` means "this engine does not score its output", which is not the
    /// same as a perfect score: Windows.Media.Ocr returns plain text and has
    /// nothing to report, and reporting 1.0 there would be a nicer number that is
    /// a lie. Only the comparison command needs this; indexing does not.
    fn lines_with_scores(&self, path: &Path) -> Result<Vec<(String, Option<f32>)>> {
        Ok(self
            .recognize(path)?
            .lines()
            .map(|line| (line.to_string(), None))
            .collect())
    }
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
///
/// `models` is here because an engine's availability is partly a question about
/// what is installed: the ONNX engine needs its 23 MB of verified weights before
/// it can be chosen at all, and "is OCR available?" is not answerable without
/// asking.
pub fn select(choice: &OcrChoice, models: &Models) -> Result<EngineSelection> {
    match choice {
        OcrChoice::Native => Ok(EngineSelection::of(default_engine(models))),
        OcrChoice::Named(name) => Ok(EngineSelection::of(named_engine(name, models)?)),
    }
}

/// The name of this platform's own OCR engine, or `None` if it has none.
///
/// Windows and macOS each have one; Linux has no system OCR at all, which is
/// why the cross-platform ONNX engine exists. `None` is not a failure: it is
/// what makes the ONNX engine the default on Linux.
pub fn platform_engine_name() -> Option<&'static str> {
    #[cfg(windows)]
    {
        Some("windows")
    }
    #[cfg(target_os = "macos")]
    {
        Some("macos")
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        None
    }
}

/// The name of the engine this machine uses by default, even when no engine is
/// available — so diagnostics can say "none" rather than failing to name it.
pub fn default_engine_name() -> &'static str {
    platform_engine_name().unwrap_or("onnx")
}

/// Which engine to use, given what is available, in preference order.
///
/// Pure, and that is the point: the preference has to hold on a CI runner with
/// no OCR language pack and on a developer's Windows machine that has one, and
/// the only way to test that on one machine is to take the availability as an
/// argument rather than as something the machine decides.
///
/// ADR-0008: the platform engine is tens of times faster, so it wins whenever it
/// exists; ONNX is the fallback and needs no language pack.
pub fn preferred_engine(platform_available: bool, onnx_available: bool) -> Option<&'static str> {
    if platform_available {
        platform_engine_name()
    } else if onnx_available {
        Some("onnx")
    } else {
        None
    }
}

/// The best available engine on this machine, in the order [`preferred_engine`]
/// sets out: the operating system's own engine first because it is tens of times
/// faster, then the ONNX fallback, then an engine that explains itself per file.
///
/// Not a hard error: the image pipeline records a failed file either way, and a
/// machine with neither engine should get one clear message per file rather than
/// a refusal to start indexing at all.
pub fn default_engine(models: &Models) -> Box<dyn OcrEngine> {
    #[cfg(windows)]
    let platform = windows::WindowsOcr::new().is_ok();
    #[cfg(target_os = "macos")]
    let platform = vision::VisionOcr::new().is_ok();
    #[cfg(not(any(windows, target_os = "macos")))]
    let platform = false;
    // Loading the ONNX engine is the expensive way to ask whether it is
    // available, so the cheap question is asked first and the answer reused.
    let onnx_installed = onnx::OnnxOcr::is_installed(models);

    match preferred_engine(platform, onnx_installed) {
        Some(name) if Some(name) == platform_engine_name() => {
            // The only way to get here is a platform engine that exists, so this
            // cannot fail; if it somehow did, the fallback below still applies.
            match platform_engine() {
                Some(engine) => engine,
                None => Box::new(Unavailable(name)),
            }
        }
        Some(name) => match onnx::OnnxOcr::new(models) {
            Ok(engine) => Box::new(engine),
            Err(err) => {
                eprintln!("the onnx models are installed but the engine would not load: {err:#}");
                Box::new(Unavailable(name))
            }
        },
        None => Box::new(Unavailable(default_engine_name())),
    }
}

/// The platform's own engine, if this machine has one that works.
#[cfg(windows)]
pub(crate) fn platform_engine() -> Option<Box<dyn OcrEngine>> {
    windows::WindowsOcr::new()
        .ok()
        .map(|engine| Box::new(engine) as Box<dyn OcrEngine>)
}

/// macOS's own engine, behind the same seam as every other adapter.
#[cfg(target_os = "macos")]
pub(crate) fn platform_engine() -> Option<Box<dyn OcrEngine>> {
    vision::VisionOcr::new()
        .ok()
        .map(|engine| Box::new(engine) as Box<dyn OcrEngine>)
}

/// There is no platform engine on Linux, so the fallback is always the ONNX
/// engine and this exists only so the selection above reads the same on every
/// platform. Linux is a supported target, not a broken build: ADR-0007 puts it
/// on the cross-platform engine by design.
#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn platform_engine() -> Option<Box<dyn OcrEngine>> {
    None
}

/// Whether this machine can recognise text at all. Cheap enough to ask before a
/// long run, so "every screenshot came back empty" is explained up front instead
/// of after an hour of indexing.
pub fn is_available(models: &Models) -> bool {
    #[cfg(windows)]
    if windows::WindowsOcr::new().is_ok() {
        return true;
    }
    #[cfg(target_os = "macos")]
    if vision::VisionOcr::new().is_ok() {
        return true;
    }
    onnx::OnnxOcr::is_installed(models)
}

/// How to get OCR working, said once so the advice cannot drift between the
/// adapter that reports a missing language pack and the engine that stands in
/// for it.
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
        // The advice is platform-specific: telling a Linux user to open Windows
        // Settings sends them looking for a control panel that does not exist,
        // which is worse than saying nothing. Both routes are named, because the
        // cross-platform one works everywhere and needs no OS setup at all.
        //
        // macOS has no branch on purpose: Vision ships with the OS, so there is
        // no pack to install and any advice about one would be fiction.
        let platform_help = match platform_engine_name() {
            Some("windows") => format!("Either {WINDOWS_LANGUAGE_PACK_HELP}."),
            Some("macos") => {
                "macOS Vision ships with the system, so there is nothing to install.".to_string()
            }
            _ => "Either install an OCR language pack for your platform.".to_string(),
        };
        anyhow::bail!(
            "no {} OCR engine is available on this machine. {platform_help} Or install the \
             cross-platform engine, which needs no language pack and works on every platform: \
             tessera models install --set {}",
            self.0,
            onnx::MODEL_SET
        )
    }
}

fn named_engine(name: &str, models: &Models) -> Result<Box<dyn OcrEngine>> {
    match name {
        #[cfg(windows)]
        "windows" => Ok(Box::new(windows::WindowsOcr::new()?)),
        #[cfg(not(windows))]
        "windows" => anyhow::bail!("the Windows OCR engine does not exist on this platform"),
        #[cfg(target_os = "macos")]
        "macos" => Ok(Box::new(vision::VisionOcr::new()?)),
        #[cfg(not(target_os = "macos"))]
        "macos" => anyhow::bail!("the macOS Vision engine does not exist on this platform"),
        "onnx" => Ok(Box::new(onnx::OnnxOcr::new(models)?)),
        other => {
            // The list names the engines *this build* has, so the error cannot
            // send someone to a `--ocr-engine` value that would fail for a
            // reason they would not guess.
            anyhow::bail!(
                "no OCR engine named {other:?} ({})",
                available_engine_names()
            )
        }
    }
}

/// The engines this build can name, for diagnostics.
fn available_engine_names() -> String {
    let mut names = vec!["onnx"];
    if cfg!(windows) {
        names.push("windows");
    }
    if cfg!(target_os = "macos") {
        names.push("macos");
    }
    names.join(", ")
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

    /// A models directory with nothing in it, which is what a machine that has
    /// never run `tessera models install` looks like. Selection tests must not
    /// depend on whether the developer already has the 23 MB of weights.
    fn empty_models() -> Models {
        let dir =
            std::env::temp_dir().join(format!("tessera-ocr-no-models-{}", std::process::id()));
        Models::open(&dir).expect("an empty models dir is always openable")
    }

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
        // Not "no text found": an engine that returns nothing looks like an index
        // that cannot find anything, which is the hardest failure to debug.
        // The message names which engine was missing: a user who passed
        // `--ocr-engine onnx` needs to know it was the fallback that was absent,
        // not their platform engine.
        assert!(
            message.contains("no onnx OCR engine is available"),
            "the message does not name the engine: {message}"
        );
        // And it says what would fix it, on both platforms.
        assert!(message.contains("language pack"), "{message}");
        assert!(message.contains("tessera models install"), "{message}");
    }

    #[test]
    fn the_platform_engine_wins_whenever_it_exists() {
        // The preference order, with availability passed in, so it is the same
        // test on a CI runner with no language pack and on a machine with one.
        assert_eq!(preferred_engine(true, true), platform_engine_name());
        assert_eq!(preferred_engine(true, false), platform_engine_name());
    }

    #[test]
    fn onnx_is_used_when_there_is_no_platform_engine() {
        // The fallback the ticket asks for, and the path Linux takes.
        assert_eq!(preferred_engine(false, true), Some("onnx"));
    }

    #[test]
    fn neither_engine_gives_no_preference_rather_than_a_wrong_one() {
        assert_eq!(preferred_engine(false, false), None);
    }

    #[test]
    fn an_unknown_engine_name_is_named_in_the_error() {
        let Err(err) = named_engine("tesseract", &empty_models()) else {
            panic!("an engine we do not have must not resolve");
        };
        assert!(format!("{err:#}").contains("tesseract"));
    }

    #[test]
    fn the_native_choice_always_resolves_to_something_that_names_itself() {
        // On a machine with no OCR models this resolves to Unavailable, which is
        // deliberate: selection must not fail, because the pipeline's job is to
        // record failures per file rather than refuse to start.
        let selected = select(&OcrChoice::Native, &empty_models()).unwrap();
        assert_eq!(selected.name, selected.engine.name());
    }

    #[test]
    fn a_named_choice_keeps_the_name_it_was_given() {
        #[cfg(windows)]
        {
            let selected = select(&OcrChoice::Named("windows".into()), &empty_models()).unwrap();
            assert_eq!(selected.name, "windows");
            assert_eq!(selected.name, selected.engine.name());
        }
        // An engine that does not exist here is an error, not a silent
        // substitution for something else.
        let Err(err) = select(&OcrChoice::Named("nope".into()), &empty_models()) else {
            panic!("an engine we do not have must not resolve");
        };
        assert!(format!("{err:#}").contains("nope"));
    }

    #[test]
    fn the_cross_platform_engine_refuses_to_run_without_its_models() {
        // Asking for it by name when it is not installed must say how to install
        // it, rather than failing with a missing-file error from a graph loader.
        let Err(err) = named_engine("onnx", &empty_models()) else {
            panic!("the engine must not exist without its models");
        };
        let message = format!("{err:#}");
        assert!(message.contains("models install --set ppocr"), "{message}");
    }

    #[test]
    fn this_builds_platform_engine_name_matches_the_platform_it_is_on() {
        // The whole of ticket 06's cross-platform claim in one assertion: the
        // name has to be right *here*, because it is the string that gets
        // recorded against a tile and the string a user passes to
        // `--ocr-engine`. On a machine with no OCR configured it still resolves,
        // because selection must name an engine rather than fail.
        let name = platform_engine_name();
        if cfg!(windows) {
            assert_eq!(name, Some("windows"));
        } else if cfg!(target_os = "macos") {
            assert_eq!(name, Some("macos"));
        } else {
            assert_eq!(
                name, None,
                "Linux has no system OCR by design (ADR-0007); the ONNX engine is its default"
            );
        }
        // The default name is always something, even with no engines at all.
        assert!(!default_engine_name().is_empty());
    }

    #[test]
    fn the_engine_list_in_the_error_names_only_engines_this_build_has() {
        // A `--ocr-engine` value suggested by an error that this build rejects
        // is worse than no suggestion, so the list is generated from the same
        // cfgs that resolve the names.
        let Err(err) = named_engine("tesseract", &empty_models()) else {
            panic!("an engine we do not have must not resolve");
        };
        let message = format!("{err:#}");
        assert!(message.contains("onnx"), "{message}");
        if cfg!(windows) {
            assert!(message.contains("windows"), "{message}");
        }
        if cfg!(target_os = "macos") {
            assert!(message.contains("macos"), "{message}");
        }
        if !cfg!(windows) {
            assert!(
                !message.contains("windows"),
                "a non-Windows build should not offer the Windows engine: {message}"
            );
        }
    }

    #[test]
    fn the_unavailable_message_never_sends_a_mac_user_to_windows_settings() {
        // The advice is platform-specific and getting it wrong sends someone
        // looking for a control panel that does not exist on their machine —
        // which is worse than saying nothing. macOS ships Vision with the OS,
        // so it gets no language-pack advice at all.
        let err = match Unavailable("macos").recognize(Path::new("anything.png")) {
            Err(err) => err,
            Ok(_) => panic!("an unavailable engine must not return text"),
        };
        let message = format!("{err:#}");
        if cfg!(target_os = "macos") {
            // Asserted against the *instruction*, not the word. The message's
            // tail legitimately says the cross-platform engine "needs no
            // language pack", so a bare substring check would fail here for a
            // reason that has nothing to do with the advice being wrong.
            assert!(
                !message.contains("install an OCR language pack"),
                "macOS was told to install a language pack: {message}"
            );
            assert!(!message.contains("Settings >"), "{message}");
            assert!(message.contains("ships with the system"), "{message}");
        } else {
            assert!(message.contains("language pack"), "{message}");
        }
        // Either way it names the cross-platform route, which works everywhere.
        assert!(message.contains("tessera models install"), "{message}");
    }
}
