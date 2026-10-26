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
//!   Windows renders. The pixels never pass through our address space — except
//!   for a capture taller or wider than the recogniser's own limit, which the
//!   OS refuses outright and which is read band by band instead.
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
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, SoftwareBitmap,
};

use super::{lines_into_text, OcrEngine};

/// Blank rows added above and below every band before recognition.
///
/// Windows OCR silently drops a line that touches the bottom of the crop.
/// `band_rows` cuts at the first quiet row, so without a margin the last line
/// above a cut has zero blank rows inside the band — the round-5 seam repro
/// read 2 of 5 lines and recovered the lost ones with 40–60 px of padding.
/// 64 px is just above that and about two line heights at the gate's capture
/// scale. The band-content limit shrinks by twice this, so a padded band still
/// fits `OcrEngine::MaxImageDimension`.
const BAND_MARGIN: u32 = 64;

/// Windows' own text recogniser.
pub struct WindowsOcr {
    engine: windows::Media::Ocr::OcrEngine,
    /// One `RecognizeAsync` at a time, per Windows' own rule.
    ///
    /// `OcrEngine` is documented as agile and the trait is `Send + Sync`, but the
    /// recogniser rejects a second call while one is in flight —
    /// `Another RecognizeAsync operation is already running! (0x80004004)`. The
    /// indexing queue runs up to four workers over one engine, so without this
    /// the first 5,001-file gate run lost 28 distinct files (42 attempts, some
    /// retried successfully) to a timing collision. The mutex is around the
    /// recognition only: decoding and the SigLIP embedding either side of it
    /// still overlap, and the recogniser was the serial resource anyway.
    recognizing: std::sync::Mutex<()>,
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
        Ok(Self {
            engine,
            recognizing: std::sync::Mutex::new(()),
        })
    }
}
impl OcrEngine for WindowsOcr {
    fn name(&self) -> &'static str {
        "windows"
    }

    fn recognize(&self, path: &Path) -> Result<String> {
        // Windows imaging wants an absolute, normalised path and answers
        // anything else with an error that says nothing useful. Normalising
        // here means every caller can pass what it has — a joined relative
        // path, a mixed-separator path from a walk — and get the same result.
        let path =
            dunce::canonicalize(path).with_context(|| format!("opening {}", path.display()))?;

        // The dimensions are header-only, so the fast path still never hands
        // the pixels to us.
        let (decoder, width, height) = open(&path)?;

        let lines = match image_limit() {
            Some(max) if width > max || height > max => self.read_in_bands(&path, max)?,
            _ => self.read_bitmap(&software_bitmap(&decoder, &path)?, &path)?,
        };
        Ok(lines_into_text(&lines))
    }
}

impl WindowsOcr {
    /// One recognition, serialised against every other worker.
    ///
    /// A poisoned lock means a previous recognition panicked; the recogniser
    /// itself is still fine, so the guard is recovered instead of turning one
    /// bad file into an engine that never reads another.
    fn read_bitmap(&self, bitmap: &SoftwareBitmap, path: &Path) -> Result<Vec<String>> {
        let _recognizing = self
            .recognizing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let recognize_op = self
            .engine
            .RecognizeAsync(bitmap)
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
        Ok(text)
    }

    /// Read an image the recogniser refuses whole.
    ///
    /// `OcrEngine::MaxImageDimension` is a hard limit: a full-page capture
    /// 21,142 pixels tall came back as "Image dimensions are too large! Check
    /// MaxImageDimension", which put a perfectly readable screenshot in the
    /// failed column with no text and no description. Banding is the fix, and
    /// it keeps the OS as the reader — every band is a normal WinRT bitmap.
    ///
    /// A capture wider than the limit cannot be banded usefully (a vertical
    /// cut severs every line), so it is scaled to the limit first; that loses
    /// resolution but keeps the file readable, and it is rare enough that the
    /// scaled path is not worth a second strategy.
    fn read_in_bands(&self, path: &Path, max: u32) -> Result<Vec<String>> {
        let mut pixels = image::open(path)
            .with_context(|| format!("decoding {}", path.display()))?
            .into_rgb8();
        if pixels.width() > max {
            let height = (u64::from(pixels.height()) * u64::from(max) / u64::from(pixels.width()))
                .max(1) as u32;
            pixels = image::imageops::resize(
                &pixels,
                max,
                height,
                image::imageops::FilterType::Triangle,
            );
        }
        let bands = band_rows(&pixels, max);
        // Padding is only needed where there is a cut. A single band is the
        // whole (possibly width-scaled) image, which is already inside the
        // recogniser's limit; adding rows could push it back over.
        let margin = if bands.len() > 1 { band_margin(max) } else { 0 };
        let mut lines = Vec::new();
        for (top, bottom) in bands {
            let bitmap = band_bitmap(&pixels, top, bottom, margin)?;
            lines.extend(self.read_bitmap(&bitmap, path)?);
        }
        Ok(lines)
    }
}

/// The recogniser's own limit, or `None` when this Windows will not say.
fn image_limit() -> Option<u32> {
    windows::Media::Ocr::OcrEngine::MaxImageDimension()
        .ok()
        .filter(|max| *max > 0)
}

/// Open the file through Windows imaging and read its header.
fn open(path: &Path) -> Result<(BitmapDecoder, u32, u32)> {
    use windows::Storage::FileAccessMode;

    let file_op = windows::Storage::StorageFile::GetFileFromPathAsync(&to_hstring(path))
        .with_context(|| format!("opening {}", path.display()))?;
    let file = block_on(file_op).with_context(|| format!("opening {}", path.display()))?;
    let stream_op = file
        .OpenAsync(FileAccessMode::Read)
        .with_context(|| format!("reading {}", path.display()))?;
    let stream = block_on(stream_op).with_context(|| format!("reading {}", path.display()))?;
    // Let Windows do what it does best: decode the file. Requesting BGRA8 up
    // front means no colour conversion happens on our thread either.
    let decoder_op = BitmapDecoder::CreateAsync(&stream)
        .with_context(|| format!("decoding {}", path.display()))?;
    let decoder = block_on(decoder_op).with_context(|| format!("decoding {}", path.display()))?;
    let width = decoder
        .PixelWidth()
        .with_context(|| format!("decoding {}", path.display()))?;
    let height = decoder
        .PixelHeight()
        .with_context(|| format!("decoding {}", path.display()))?;
    Ok((decoder, width, height))
}

fn software_bitmap(decoder: &BitmapDecoder, path: &Path) -> Result<SoftwareBitmap> {
    let bitmap_op =
        decoder.GetSoftwareBitmapConvertedAsync(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Ignore);
    let bitmap_op = bitmap_op.with_context(|| format!("decoding {}", path.display()))?;
    block_on(bitmap_op).with_context(|| format!("decoding {}", path.display()))
}

/// One band as a WinRT bitmap, built straight into the pixel format the
/// recogniser wants. Opaque, because the recogniser only reads the text.
///
/// `margin` blank rows are added above and below the band's content. The band
/// content never exceeds `max - 2 * band_margin(max)`, so the padded bitmap is
/// still within the recogniser's limit.
fn band_bitmap(
    pixels: &image::RgbImage,
    top: u32,
    bottom: u32,
    margin: u32,
) -> Result<SoftwareBitmap> {
    use windows::Storage::Streams::DataWriter;

    let width = pixels.width();
    let height = bottom - top + 2 * margin;
    let raw = pixels.as_raw();
    let mut bgra = Vec::with_capacity(width as usize * height as usize * 4);
    let blank_row = vec![0xFFu8; width as usize * 4];
    for _ in 0..margin {
        bgra.extend_from_slice(&blank_row);
    }
    for y in top..bottom {
        let row = &raw[y as usize * width as usize * 3..];
        for x in 0..width as usize {
            let rgb = &row[x * 3..x * 3 + 3];
            bgra.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 0xFF]);
        }
    }
    for _ in 0..margin {
        bgra.extend_from_slice(&blank_row);
    }

    let writer = DataWriter::new().context("preparing a recognised band")?;
    writer
        .WriteBytes(&bgra)
        .context("preparing a recognised band")?;
    let buffer = writer
        .DetachBuffer()
        .context("preparing a recognised band")?;
    SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        width as i32,
        height as i32,
        BitmapAlphaMode::Ignore,
    )
    .context("preparing a recognised band")
}

/// Split an image into horizontal bands no taller than `max`, cutting at the
/// row that looks most like blank space.
///
/// A band boundary through a line of text costs that line on both sides of the
/// seam, so the cut is not taken at the arithmetic midpoint: anywhere between
/// half a band and a full band is legal, and the quietest row in that window
/// wins. Every row inked — a dense table, a photograph — still yields valid
/// bands; the seam is then merely the best of a bad set rather than a broken
/// image.
fn band_rows(pixels: &image::RgbImage, max: u32) -> Vec<(u32, u32)> {
    let height = pixels.height();
    let max = max.max(1);
    if height <= max {
        return vec![(0, height)];
    }
    // Room for the blank margin on both sides of a recognised band. It is
    // derived from `max` so a tiny limit cannot make the padded band illegal.
    let margin = band_margin(max);
    let content = max - 2 * margin;
    let mut bands = Vec::new();
    let mut top = 0;
    while height - top > content {
        // Half a band is the floor: it bounds the number of bands and
        // guarantees the loop makes progress even on a fully inked image.
        let floor = top + content / 2 + 1;
        let ideal = top + content;
        let mut cut = ideal;
        let mut best = u32::MAX;
        for y in floor..=ideal {
            let score = transitions(pixels, y);
            if score < best {
                best = score;
                cut = y;
            }
        }
        bands.push((top, cut));
        top = cut;
    }
    bands.push((top, height));
    bands
}

/// The largest blank margin that still leaves a band one row of content.
fn band_margin(max: u32) -> u32 {
    ((max.saturating_sub(1)) / 2).min(BAND_MARGIN)
}

/// How many colour changes a row has. A blank row scores zero, a line of text
/// scores in the hundreds; that gap is what makes it a usable seam.
fn transitions(pixels: &image::RgbImage, y: u32) -> u32 {
    let width = pixels.width();
    let raw = pixels.as_raw();
    let row = &raw[y as usize * width as usize * 3..];
    let mut previous = &row[..3];
    let mut count = 0;
    for x in 1..width as usize {
        let current = &row[x * 3..x * 3 + 3];
        let delta = current
            .iter()
            .zip(previous)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        if delta > 16 {
            count += 1;
        }
        previous = current;
    }
    count
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
    fn two_workers_recognise_at_the_same_time_without_colliding() {
        // The corpus gate's 24 lost screenshots: two indexing workers called
        // RecognizeAsync on one engine and Windows refused the second with
        // "Another RecognizeAsync operation is already running!". The mutex has
        // to cover the whole operation, not only its start.
        let Ok(engine) = WindowsOcr::new() else {
            eprintln!("skipping: no Windows OCR language pack on this machine");
            return;
        };
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-sample.png");
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                barrier.wait();
                engine.recognize(&path)
            });
            let second = scope.spawn(|| {
                barrier.wait();
                engine.recognize(&path)
            });
            let first = first.join().expect("the first worker did not panic");
            let second = second.join().expect("the second worker did not panic");
            assert!(first.is_ok(), "first recognition failed: {:?}", first.err());
            assert!(
                second.is_ok(),
                "second recognition failed: {:?}",
                second.err()
            );
        });
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

    /// A white canvas with black bars from `x=5` to the right edge, standing in
    /// for lines of text: a bar has edges, which is what tells a row of type
    /// apart from a blank row.
    fn canvas(width: u32, height: u32, bars: &[(u32, u32)]) -> image::RgbImage {
        let mut image = image::RgbImage::from_pixel(width, height, image::Rgb([255, 255, 255]));
        for (top, bottom) in bars {
            for y in *top..*bottom {
                for x in 5..width.saturating_sub(5) {
                    image.put_pixel(x, y, image::Rgb([0, 0, 0]));
                }
            }
        }
        image
    }

    #[test]
    fn an_image_inside_the_limit_is_one_band() {
        let image = canvas(200, 900, &[]);
        assert_eq!(band_rows(&image, 1000), vec![(0, 900)]);
        assert_eq!(band_rows(&image, 900), vec![(0, 900)], "fit is inclusive");
    }

    #[test]
    fn bands_never_exceed_the_limit_and_cover_every_row() {
        let image = canvas(200, 1000, &[(100, 120), (400, 420), (700, 720)]);
        let bands = band_rows(&image, 300);
        assert!(bands.len() >= 4, "{bands:?}");
        assert_eq!(bands[0].0, 0, "the first band starts at the top");
        assert_eq!(
            bands.last().unwrap().1,
            1000,
            "the last band ends at the bottom"
        );
        for window in bands.windows(2) {
            assert_eq!(
                window[0].1, window[1].0,
                "bands must be contiguous: {bands:?}"
            );
        }
        for (top, bottom) in &bands {
            assert!(
                bottom - top <= 300,
                "band {top}..{bottom} is over the limit"
            );
            assert!(bottom > top, "a band must have rows");
        }
    }

    #[test]
    fn the_seam_lands_between_lines_rather_than_through_one() {
        // Every bar is a line of text. A cut inside one severs it on both
        // sides of the seam, which is the whole reason the cut is not taken at
        // the arithmetic midpoint.
        let bars = [(140, 160), (240, 260), (340, 360), (440, 460)];
        let image = canvas(200, 700, &bars);
        let bands = band_rows(&image, 300);
        for (_, cut) in &bands[..bands.len() - 1] {
            for (top, bottom) in bars {
                assert!(
                    !(top <= *cut && *cut < bottom),
                    "the seam at {cut} cuts the line at {top}..{bottom}: {bands:?}"
                );
            }
        }
    }

    #[test]
    fn a_fully_inked_image_still_bands() {
        // A photograph or a dense table has no blank row to cut on. The image
        // must still be readable, so a bad seam is better than no attempt.
        let image = image::RgbImage::from_pixel(200, 1000, image::Rgb([0, 0, 0]));
        let bands = band_rows(&image, 300);
        assert_eq!(bands[0].0, 0);
        assert_eq!(bands.last().unwrap().1, 1000);
        for (top, bottom) in &bands {
            assert!(
                bottom - top <= 300,
                "band {top}..{bottom} is over the limit"
            );
        }
    }

    /// The rows of `image` that hold ink, outside the fixture's border.
    fn ink_rows(image: &image::RgbImage) -> Vec<(u32, u32)> {
        let mut runs = Vec::new();
        let mut current: Option<u32> = None;
        for y in 0..image.height() {
            let inked = (30..image.width().saturating_sub(30)).any(|x| {
                let pixel = image.get_pixel(x, y);
                pixel[0] < 128 && pixel[1] < 128 && pixel[2] < 128
            });
            match (inked, current) {
                (true, None) => current = Some(y),
                (false, Some(top)) => {
                    runs.push((top, y - 1));
                    current = None;
                }
                _ => {}
            }
        }
        if let Some(top) = current {
            runs.push((top, image.height() - 1));
        }
        runs
    }

    /// A line that ends on the row immediately before a band's cut must still
    /// be read.
    ///
    /// This is the round-5 seam defect: `band_bitmap`'s bottom is exclusive, so
    /// the first quiet row becomes the cut and the line above it has zero blank
    /// rows inside the crop. Windows OCR silently drops a line touching its
    /// bottom edge, and the old geometric seam test could not see it because it
    /// never recognised a band. Here the fixture's last text line is placed so
    /// its final inked row is exactly `cut - 1`; the assertion is on recovered
    /// text, not on where the cut landed.
    #[test]
    fn a_line_flush_to_a_band_cut_is_still_read() {
        let Ok(engine) = WindowsOcr::new() else {
            eprintln!("skipping: no Windows OCR language pack on this machine");
            return;
        };
        let Some(max) = image_limit() else {
            eprintln!("skipping: this Windows will not report its OCR image limit");
            return;
        };
        if !(400..=20_000).contains(&max) {
            eprintln!("skipping: a {max}-pixel limit is not a useful banding fixture");
            return;
        }

        let fixture = image::open(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-sample.png"),
        )
        .expect("the fixture is readable")
        .into_rgb8();
        let (line_top, line_bottom) = *ink_rows(&fixture)
            .last()
            .expect("the fixture has a last line of text");
        // The text only, with the border and the lines above it left behind.
        let line = image::imageops::crop_imm(
            &fixture,
            30,
            line_top,
            fixture.width() - 60,
            line_bottom - line_top + 1,
        )
        .to_image();

        // The first cut is the first quiet row in `[floor, top + content]`.
        // With nothing but this line inked, that is `floor`. Placing the line's
        // last row at `floor - 1` makes it flush with the band's exclusive
        // bottom; the padding is what gives it a blank row inside the band.
        let content = max - 2 * band_margin(max);
        let cut = content / 2 + 1;
        let height = max * 2 + 200;
        let mut tall =
            image::RgbImage::from_pixel(fixture.width(), height, image::Rgb([255, 255, 255]));
        let paste_top = cut - line.height();
        image::imageops::replace(&mut tall, &line, 30, i64::from(paste_top));

        let bands = band_rows(&tall, max);
        assert_eq!(
            bands[0],
            (0, cut),
            "the fixture must exercise the first cut: {bands:?}"
        );
        assert!(
            transitions(&tall, cut - 1) > 0,
            "the line must end on the row before the cut"
        );
        assert_eq!(transitions(&tall, cut), 0, "the cut row itself is blank");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("flush.png");
        tall.save(&path).expect("the tall capture is writable");

        let text = engine
            .recognize(&path)
            .expect("a capture over the recogniser's limit must still be read")
            .to_lowercase();
        assert!(
            text.contains("issued"),
            "the line flush to the cut at row {cut} was dropped: {text:?}"
        );
    }

    #[test]
    fn a_tall_screenshot_is_read_band_by_band() {
        // The gate's real failure: a 1440x21142 full-page capture came back as
        // "Image dimensions are too large!". One recognition would refuse it,
        // so the fixture is pasted into the *bottom* band — a reader that only
        // sees the first band finds nothing at all.
        let Ok(engine) = WindowsOcr::new() else {
            eprintln!("skipping: no Windows OCR language pack on this machine");
            return;
        };
        let Some(max) = image_limit() else {
            eprintln!("skipping: this Windows will not report its OCR image limit");
            return;
        };
        if max > 20_000 {
            eprintln!("skipping: a {max}-pixel limit is too tall to outgrow in a unit test");
            return;
        }

        let fixture = image::open(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-sample.png"),
        )
        .expect("the fixture is readable")
        .into_rgb8();
        let height = max * 2 + 200;
        let mut tall =
            image::RgbImage::from_pixel(fixture.width(), height, image::Rgb([255, 255, 255]));
        let top = height - fixture.height() - 20;
        image::imageops::replace(&mut tall, &fixture, 0, i64::from(top));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tall.png");
        tall.save(&path).expect("the tall capture is writable");

        let text = engine
            .recognize(&path)
            .expect("a capture over the recogniser's limit must still be read")
            .to_lowercase();
        for expected in ["invoice", "tessera", "qf-3391"] {
            assert!(
                text.contains(expected),
                "the bottom band was not read: {expected:?} missing from {text:?}"
            );
        }
    }
}
