//! The Windows OCR adapter: `Windows.Media.Ocr`, the engine that ships with
//! Windows.
//!
//! This is the only file in the engine that knows WinRT exists. Everything else
//! talks to [`OcrEngine`], which is what makes the macOS adapter (ticket 06)
//! and the ONNX fallback (ticket 05) additions rather than a rewrite.
//!
//! Two things this adapter gets right, both from the spike's hard-won notes:
//!
//! - **The OS decodes the image.** Windows imaging reads the file natively,
//!   which is faster than any crate we could use and understands every format
//!   Windows renders. The pixels never pass through our address space.
//! - **Every WinRT call is asynchronous**, and the engine runs on an indexing
//!   thread. It blocks on the operation rather than blocking a UI thread, which
//!   is why this adapter owns no async runtime either — the engine is
//!   synchronous all the way down, and one small block-on is cheaper than a
//!   runtime per worker.

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use anyhow::{Context as _, Result};

use super::{lines_into_text, OcrEngine};

/// Windows' own text recogniser.
pub struct WindowsOcr {
    engine: windows::Media::Ocr::OcrEngine,
}

impl WindowsOcr {
    /// The recogniser for the user's own languages, which is the one they expect
    /// screenshots to be read in.
    ///
    /// Fails when no OCR language pack is installed. That is reported rather
    /// than worked around: an engine that quietly returns nothing for every
    /// screenshot produces an index that looks built and finds nothing.
    pub fn new() -> Result<Self> {
        let engine = windows::Media::Ocr::OcrEngine::TryCreateFromUserProfileLanguages()
            .with_context(|| {
                format!(
                    "Windows has no OCR engine for the user's languages. To fix it, {}.",
                    super::WINDOWS_LANGUAGE_PACK_HELP
                )
            })?;
        Ok(Self { engine })
    }
}

impl OcrEngine for WindowsOcr {
    fn name(&self) -> &'static str {
        "windows"
    }

    fn recognize(&self, path: &Path) -> Result<String> {
        use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat};
        use windows::Storage::FileAccessMode;

        // Windows imaging wants an absolute, normalised path and answers
        // anything else with an error that says nothing useful. Normalising
        // here means every caller can pass what it has — a joined relative
        // path, a mixed-separator path from a walk — and get the same result.
        let path =
            dunce::canonicalize(path).with_context(|| format!("opening {}", path.display()))?;

        let file_op = windows::Storage::StorageFile::GetFileFromPathAsync(&to_hstring(&path))
            .with_context(|| format!("opening {}", path.display()))?;
        let file = block_on(file_op).with_context(|| format!("opening {}", path.display()))?;
        let stream_op = file
            .OpenAsync(FileAccessMode::Read)
            .with_context(|| format!("reading {}", path.display()))?;
        let stream = block_on(stream_op).with_context(|| format!("reading {}", path.display()))?;
        // Let Windows do what it does best: decode the file. Requesting BGRA8
        // up front means no colour conversion happens on our thread either.
        let decoder_op = BitmapDecoder::CreateAsync(&stream)
            .with_context(|| format!("decoding {}", path.display()))?;
        let decoder =
            block_on(decoder_op).with_context(|| format!("decoding {}", path.display()))?;
        let bitmap_op = decoder
            .GetSoftwareBitmapConvertedAsync(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Ignore);
        let bitmap_op = bitmap_op.with_context(|| format!("decoding {}", path.display()))?;
        let bitmap = block_on(bitmap_op).with_context(|| format!("decoding {}", path.display()))?;

        let recognize_op = self
            .engine
            .RecognizeAsync(&bitmap)
            .with_context(|| format!("reading the text in {}", path.display()))?;
        let result = block_on(recognize_op)
            .with_context(|| format!("reading the text in {}", path.display()))?;

        // `Lines` is already in reading order, which is what a search index
        // wants and what walking words positionally throws away.
        let lines = result
            .Lines()
            .context("the recognised result had no lines")?;
        let count = lines.Size().context("counting the recognised lines")?;
        let mut text = Vec::with_capacity(count as usize);
        for i in 0..count {
            let line = lines.GetAt(i).context("reading a recognised line")?;
            text.push(
                line.Text()
                    .context("reading a recognised line")?
                    .to_string(),
            );
        }
        Ok(lines_into_text(&text))
    }
}

fn to_hstring(path: &Path) -> windows::core::HSTRING {
    use std::os::windows::ffi::OsStrExt;
    windows::core::HSTRING::from_wide(&path.as_os_str().encode_wide().collect::<Vec<u16>>())
}

/// Block until a WinRT operation finishes.
///
/// The engine is synchronous end to end, and pulling in an async runtime to
/// await one operation per image would be the larger cost. Windows completes
/// the operation on its own thread and wakes us through the future's waker, so
/// parking until then is both correct and free.
fn block_on<F: std::future::IntoFuture>(future: F) -> F::Output {
    struct ThreadWaker(std::thread::Thread);
    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let future = future.into_future();
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_name_is_windows() {
        // Naming itself is what puts "windows" on the file row. No engine
        // needed: the name is a constant of the adapter, not of the machine.
        assert_eq!(crate::ocr::Unavailable("windows").name(), "windows");
        if let Ok(engine) = WindowsOcr::new() {
            assert_eq!(engine.name(), "windows");
        }
    }

    #[test]
    fn a_path_becomes_a_windows_string() {
        assert_eq!(
            to_hstring(Path::new(r"C:\a\b.png")).to_string(),
            r"C:\a\b.png"
        );
    }

    /// The end-to-end proof that the adapter works, skipped where it cannot: a
    /// machine with no OCR language pack installed. Where it can run, it runs —
    /// a test that only ever skips is how an adapter ships broken.
    #[test]
    fn a_real_screenshot_yields_its_text() {
        let Ok(engine) = WindowsOcr::new() else {
            eprintln!("skipping: no Windows OCR language pack on this machine");
            return;
        };
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-sample.png");
        let text = engine.recognize(&path).expect("the fixture is readable");
        let text = text.to_lowercase();
        for expected in ["invoice", "tessera", "qf-3391"] {
            assert!(
                text.contains(expected),
                "the fixture should contain {expected:?}, got {text:?}"
            );
        }
    }

    #[test]
    fn a_file_that_is_not_an_image_is_an_error_naming_it() {
        let Ok(engine) = WindowsOcr::new() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-an-image.png");
        std::fs::write(&path, b"this is not a png").unwrap();
        let err = engine.recognize(&path).unwrap_err();
        let message = format!("{err:#}");
        assert!(
            message.contains("not-an-image.png"),
            "the failure must name the file: {message}"
        );
    }

    #[test]
    fn a_file_that_is_not_there_is_an_error() {
        let Ok(engine) = WindowsOcr::new() else {
            return;
        };
        let err = engine
            .recognize(Path::new(r"C:\nope\missing.png"))
            .unwrap_err();
        assert!(format!("{err:#}").contains("missing.png"));
    }
}
