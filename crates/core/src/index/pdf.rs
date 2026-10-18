//! The PDF pipeline: the text layer, page by page.
//!
//! PDFs split along an axis no other format has: pages. A search result that
//! says "page 4" is actionable and a chunk number is not, so the page rides on
//! the tile ([`Tile::page`]) from the moment the text is read, rather than
//! being guessed later from tile position. Within a page the text chunks like
//! any other document, because a PDF page can be a whole book chapter and the
//! search model has an input limit.
//!
//! A PDF with no text layer anywhere — a scan — is not a failure and not an
//! empty document: it is [`Extraction::AwaitingOcr`], recorded by the index
//! loop as waiting for the OCR pass the spec schedules for v0.1.x. The bytes
//! were read; there is simply no text layer this build can read out of them.

use std::path::Path;

use anyhow::{Context, Result};

use super::text::chunk_text;
use super::{Extraction, Kind, Pipeline, Tile};

const EXTENSIONS: &[&str] = &["pdf"];

/// PDFs: the text layer, one chunk of one page per tile.
pub struct PdfPipeline;

impl Pipeline for PdfPipeline {
    fn accepts(&self, path: &Path) -> Option<Kind> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())?
            .to_ascii_lowercase();
        EXTENSIONS.contains(&ext.as_str()).then_some(Kind::Pdf)
    }

    fn process(&self, path: &Path) -> Result<Extraction> {
        let pages = extract_pages(path)?;
        Ok(tiles_from_pages(&pages))
    }
}

/// The text of each page, in order, one entry per page.
///
/// `pdf-extract` walks page numbers from 1 while they resolve, and it pushes a
/// page's text even when that text is empty — so entry `i` is page `i + 1`,
/// and blank pages keep their slot instead of shifting later pages down. That
/// is what makes "page 4" mean page 4.
///
/// The one thing this cannot see: `pdf-extract` stops at the first page whose
/// content stream it refuses, so a document with a damaged middle page is
/// extracted up to that page and no further. Diagnosing it would mean a second
/// full parse through a lower-level library — doubling the cost of every
/// extraction to find a case a later diagnostics pass can re-check on its own.
///
/// The parser meets arbitrary files from the internet and from users, and
/// "one bad file never stops the run" (`index_folder`) has to hold for bytes,
/// not only for formats: a panic inside extraction becomes this file's
/// failure, not the run's death. The closure captures only `path`, so
/// resuming after it unwinds is sound.
fn extract_pages(path: &Path) -> Result<Vec<String>> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_extract::extract_text_by_pages(path)
    })) {
        Ok(result) => result.with_context(|| format!("reading {}", path.display())),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "no message".to_string());
            anyhow::bail!(
                "reading {}: the PDF parser panicked ({message}); the file is recorded as \
                 failed and the run continues",
                path.display()
            )
        }
    }
}

/// Per-page text into tiles: every tile keeps the page it came from, and the
/// text of one page chunks like every other document.
///
/// `AwaitingOcr` only when *no* page carried text at all. One readable page is
/// a document, and its words must not be thrown away because page 3 is a
/// picture.
fn tiles_from_pages(pages: &[String]) -> Extraction {
    let mut tiles = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        for chunk in chunk_text(page) {
            tiles.push(Tile {
                seq: tiles.len() as u32,
                text: chunk,
                page: Some(index as u32 + 1),
            });
        }
    }
    if tiles.is_empty() {
        Extraction::AwaitingOcr
    } else {
        Extraction::Tiles(tiles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts(name: &str) -> Option<Kind> {
        PdfPipeline.accepts(Path::new(name))
    }

    #[test]
    fn claims_pdfs_and_names_the_kind() {
        assert_eq!(accepts("a.pdf"), Some(Kind::Pdf));
        assert_eq!(accepts("b.PDF"), Some(Kind::Pdf)); // case-insensitive
        assert_eq!(accepts("c.docx"), None);
        assert_eq!(accepts("d.txt"), None);
        assert_eq!(accepts("noext"), None);
    }

    #[test]
    fn every_tile_carries_its_page() {
        let pages = vec![
            "cover page".to_string(),
            String::new(), // a blank page keeps its slot
            "aardvark on page three".to_string(),
        ];
        let Extraction::Tiles(tiles) = tiles_from_pages(&pages) else {
            panic!("pages with text are not awaiting OCR");
        };
        let pages: Vec<Option<u32>> = tiles.iter().map(|t| t.page).collect();
        assert_eq!(
            pages,
            vec![Some(1), Some(3)],
            "the blank page 2 must not pull page 3 down to 2"
        );
        assert_eq!(tiles.iter().map(|t| t.seq).collect::<Vec<_>>(), vec![0, 1]);
        assert!(tiles[1].text.contains("aardvark"));
    }

    #[test]
    fn a_long_page_chunks_without_losing_its_number() {
        let words: Vec<String> = (0..450).map(|i| format!("w{i}")).collect();
        let pages = vec![words.join(" ")];
        let Extraction::Tiles(tiles) = tiles_from_pages(&pages) else {
            panic!("a page with text is not awaiting OCR");
        };
        assert_eq!(tiles.len(), 3, "450 words chunk like any other text");
        assert!(tiles.iter().all(|t| t.page == Some(1)));
        assert_eq!(
            tiles.iter().map(|t| t.seq).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert!(tiles[1].text.starts_with("w150 "));
    }

    #[test]
    fn a_document_with_no_text_anywhere_is_awaiting_ocr() {
        let pages = vec![String::new(), "   \n\t ".to_string()];
        assert_eq!(tiles_from_pages(&pages), Extraction::AwaitingOcr);
        assert_eq!(
            tiles_from_pages(&[]),
            Extraction::AwaitingOcr,
            "a pdf where no page extracts has nothing to read either"
        );
    }

    #[test]
    fn page_numbers_restart_nothing_across_a_document() {
        let pages = vec!["one".to_string(), "two".to_string(), "three".to_string()];
        let Extraction::Tiles(tiles) = tiles_from_pages(&pages) else {
            panic!("pages with text are not awaiting OCR");
        };
        assert_eq!(
            tiles.iter().map(|t| t.page).collect::<Vec<_>>(),
            vec![Some(1), Some(2), Some(3)]
        );
        assert_eq!(
            tiles.iter().map(|t| t.seq).collect::<Vec<_>>(),
            vec![0, 1, 2],
            "seq is a document-wide position; page is the page"
        );
    }
}
