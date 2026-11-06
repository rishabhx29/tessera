//! Small pictures of indexed images, for a result list.
//!
//! This is the same job the image pipeline already does — decode a file with
//! `image` — so it lives in the engine rather than the shell. Two differences:
//! the output is a JPEG scaled to a list tile instead of pixels for OCR, and
//! decoding runs under explicit limits, because a thumbnail request may name a
//! file whose header claims enormous dimensions. A hostile or torn header is
//! an error here, never an allocation the process cannot back out of.
//!
//! Caching is the caller's business: which file version a thumbnail describes,
//! where the cache lives, and how long it is kept all depend on the index, and
//! that is `tessera-app`'s `AppService::thumbnail`.

use std::path::Path;

use anyhow::{Context, Result};

/// A cap on the pixels one decode may allocate. A 4K screenshot needs ~100 MB
/// at its widest; 256 MB leaves room for a large photo without letting a
/// crafted header ask for gigabytes.
const MAX_ALLOC_BYTES: u64 = 256 * 1024 * 1024;

/// Decode `path`, shrink its longest edge to `longest_edge`, and encode it as
/// a JPEG at `quality`.
///
/// The aspect ratio is preserved: a list tile crops with CSS (`object-fit:
/// cover`), which keeps the picture's own framing in the pixels we ship.
pub fn render_jpeg(path: &Path, longest_edge: u32, quality: u8) -> Result<Vec<u8>> {
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_ALLOC_BYTES);
    let mut reader = image::ImageReader::open(path)
        .with_context(|| format!("reading {}", path.display()))?
        .with_guessed_format()
        .with_context(|| format!("{} could not be read as an image", path.display()))?;
    reader.limits(limits);
    let image = reader
        .decode()
        .with_context(|| format!("{} could not be decoded", path.display()))?;
    // `thumbnail` scales with a fast filter and keeps the aspect ratio, which
    // is exactly a list preview: no crop, and no need for a high-quality
    // resample on a 44 px tile.
    let thumb = image.thumbnail(longest_edge.max(1), longest_edge.max(1));
    let rgb = thumb.to_rgb8();
    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .with_context(|| format!("encoding a thumbnail for {}", path.display()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_photo_is_scaled_to_the_longest_edge_and_encoded_as_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.png");
        image::RgbImage::from_pixel(1200, 800, image::Rgb([200, 120, 60]))
            .save(&path)
            .unwrap();

        let bytes = render_jpeg(&path, 320, 82).unwrap();
        assert!(
            bytes.starts_with(&[0xFF, 0xD8]),
            "a thumbnail is a JPEG, so the list can trust one MIME type"
        );
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (320, 213));
    }

    #[test]
    fn a_tall_image_is_bounded_by_the_same_edge() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tall.png");
        image::RgbImage::from_pixel(200, 1000, image::Rgb([10, 20, 30]))
            .save(&path)
            .unwrap();
        let bytes = render_jpeg(&path, 320, 82).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 320));
    }

    #[test]
    fn a_corrupt_or_missing_file_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let broken = dir.path().join("broken.png");
        std::fs::write(&broken, b"\x89PNG\r\n\x1a\n not really a picture").unwrap();
        let error = render_jpeg(&broken, 320, 82).unwrap_err();
        assert!(
            format!("{error:#}").contains("could not be decoded"),
            "{error:#}"
        );

        let missing = dir.path().join("missing.png");
        let error = render_jpeg(&missing, 320, 82).unwrap_err();
        assert!(format!("{error:#}").contains("reading"), "{error:#}");
    }
}
