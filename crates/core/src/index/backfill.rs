//! Background backfill: give old index rows the geometry they were indexed
//! without.
//!
//! Two cheap passes over existing image rows. The size pass reads each
//! picture's header, so the wall gets true aspect ratios. The box pass reads
//! the picture again only when the stored text can be matched line for line
//! against a fresh OCR read; a mismatch stores "no geometry" rather than
//! putting a mark where no word is.
//!
//! Neither pass opens a file that has become a cloud placeholder, and a
//! missing file is skipped rather than failed: the user may have deleted it
//! since the last index run, and that is not the backfill's problem.

use std::path::Path;

use anyhow::Result;

use crate::index::skip::{FileProbe, SystemProbe};
use crate::ocr::OcrEngine;
use crate::store::Store;

/// What one backfill run resolved.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BackfillSummary {
    /// Pictures whose width and height were read from their header.
    pub sized: usize,
    /// Tiles whose line geometry was decided, including "there is none".
    pub boxed: usize,
    /// Work items whose file was gone, or is a cloud placeholder, and was not
    /// opened. One file can count twice: a missing picture has both a size and
    /// a tile in the work lists.
    pub skipped: usize,
    /// Files that exist but could not be read.
    pub failed: usize,
}

/// Fill the missing picture sizes and line boxes.
///
/// Safe to call on every launch: a resolved row is not in either work list.
pub fn backfill(store: &Store, ocr: &dyn OcrEngine) -> Result<BackfillSummary> {
    let mut summary = BackfillSummary::default();
    backfill_sizes(store, &mut summary)?;
    backfill_boxes(store, ocr, &mut summary)?;
    Ok(summary)
}

/// Read the header of every picture whose size is not yet known.
fn backfill_sizes(store: &Store, summary: &mut BackfillSummary) -> Result<()> {
    for (file_id, path) in store.image_files_missing_dimensions()? {
        let path = Path::new(&path);
        match readable_locally(path) {
            None | Some(false) => {
                summary.skipped += 1;
                continue;
            }
            Some(true) => {}
        }
        match image::image_dimensions(path) {
            Ok((width, height)) => {
                store.set_dimensions(file_id, width, height)?;
                summary.sized += 1;
            }
            Err(err) => {
                eprintln!("the size of {} could not be read: {err}", path.display());
                summary.failed += 1;
            }
        }
    }
    Ok(())
}

/// Read the geometry of every picture tile that has never had it.
///
/// The stored text is the evidence that the picture being read is the one the
/// index describes: the filename line plus the body lines, one for one. A
/// different count or different text means the file changed since indexing,
/// and boxes from the new picture would be a lie about the old text. `[]`
/// records that the decision was made, so no later launch repeats it.
fn backfill_boxes(store: &Store, ocr: &dyn OcrEngine, summary: &mut BackfillSummary) -> Result<()> {
    for tile in store.image_tiles_missing_boxes()? {
        let path = Path::new(&tile.path);
        match readable_locally(path) {
            None | Some(false) => {
                summary.skipped += 1;
                continue;
            }
            Some(true) => {}
        }

        let stored_lines: Vec<&str> = tile.text.lines().skip(1).map(str::trim).collect();
        if stored_lines.is_empty() {
            // The filename is the only text: there is nothing to find on the
            // picture, and re-reading every textless photo would be the cost
            // this pass exists to avoid.
            store.set_tile_boxes(tile.tile_id, &[])?;
            summary.boxed += 1;
            continue;
        }

        match ocr.recognize_lines(path) {
            Ok(lines) => {
                let matches = lines.len() == stored_lines.len()
                    && lines
                        .iter()
                        .zip(&stored_lines)
                        .all(|(line, stored)| line.text.trim() == *stored);
                if matches {
                    let mut boxes = Vec::with_capacity(lines.len() + 1);
                    boxes.push(None);
                    boxes.extend(lines.iter().map(|line| line.bounds));
                    store.set_tile_boxes(tile.tile_id, &boxes)?;
                } else {
                    store.set_tile_boxes(tile.tile_id, &[])?;
                }
                summary.boxed += 1;
            }
            Err(err) => {
                eprintln!(
                    "the text of {} could not be read again: {err:#}",
                    path.display()
                );
                summary.failed += 1;
            }
        }
    }
    Ok(())
}

/// Whether the file can be opened without fetching bytes from the network.
///
/// A missing path is `None`: there is nothing to read, and that is a skip
/// rather than a failure.
fn readable_locally(path: &Path) -> Option<bool> {
    match SystemProbe.facts(path) {
        Ok(facts) => Some(!facts.placeholder),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;
    use crate::geometry::NormBox;
    use crate::index::skip::FileFacts;
    use crate::index::{Kind, Tile};
    use crate::ocr::OcrLine;
    use crate::store::Store;

    /// An OCR engine with a fixed answer that counts how often it was asked.
    struct Fixed {
        lines: Vec<OcrLine>,
        calls: Arc<AtomicUsize>,
    }

    impl Fixed {
        fn new(lines: Vec<OcrLine>) -> Self {
            Self {
                lines,
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl OcrEngine for Fixed {
        fn name(&self) -> &'static str {
            "fixed"
        }

        fn recognize(&self, _path: &Path) -> Result<String> {
            Ok(crate::ocr::lines_into_text(
                &self
                    .lines
                    .iter()
                    .map(|line| line.text.clone())
                    .collect::<Vec<_>>(),
            ))
        }

        fn recognize_lines(&self, _path: &Path) -> Result<Vec<OcrLine>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.lines.clone())
        }
    }

    fn boxed(x0: f32, y0: f32, x1: f32, y1: f32) -> Option<NormBox> {
        Some(NormBox { x0, y0, x1, y1 })
    }

    fn facts(path: &Path) -> FileFacts {
        let meta = std::fs::metadata(path).expect("the fixture exists");
        FileFacts {
            size: meta.len(),
            mtime: 1_000,
            placeholder: false,
        }
    }

    /// Index one image row whose tile text is exactly `text`, with no geometry,
    /// the way a row written before the geometry column looks. This is the
    /// state the backfill exists for; production never writes it now.
    fn index_without_boxes(store: &Store, root: &Path, path: &Path, text: &str) -> i64 {
        store
            .index_file(
                root,
                path,
                Kind::Image,
                &facts(path),
                None,
                &[Tile {
                    seq: 0,
                    text: text.to_string(),
                    page: None,
                    boxes: Vec::new(),
                }],
            )
            .unwrap();
        let file_id = store.file_id(path).unwrap();
        let tile_id = store.file_displays(&[file_id]).unwrap()[&file_id]
            .first_tile
            .as_ref()
            .unwrap()
            .0;
        store.forget_tile_boxes(tile_id).unwrap();
        file_id
    }

    fn boxes_of(store: &Store, file_id: i64) -> Vec<Option<NormBox>> {
        store.files_by_ids(&[file_id]).unwrap().remove(0).tiles[0]
            .boxes
            .clone()
    }

    #[test]
    fn sizes_are_filled_from_headers() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        let picture = corpus.path().join("wide.png");
        image::RgbImage::new(80, 40).save(&picture).unwrap();
        let file_id = index_without_boxes(&store, corpus.path(), &picture, "wide.png");

        // A row whose file is gone is skipped, not failed: it is not this
        // run's job to guess whether the user meant to delete it.
        let gone = corpus.path().join("gone.png");
        image::RgbImage::new(4, 4).save(&gone).unwrap();
        index_without_boxes(&store, corpus.path(), &gone, "gone.png");
        std::fs::remove_file(&gone).unwrap();

        let ocr = Fixed::new(Vec::new());
        let summary = backfill(&store, &ocr).unwrap();
        assert_eq!(
            (summary.sized, summary.skipped, summary.failed),
            (1, 2, 0),
            "the missing picture is skipped once for its size and once for its tile"
        );
        let displays = store.file_displays(&[file_id]).unwrap();
        assert_eq!(
            (displays[&file_id].width, displays[&file_id].height),
            (Some(80), Some(40))
        );

        let again = backfill(&store, &ocr).unwrap();
        assert_eq!((again.sized, again.skipped, again.failed), (0, 2, 0));
    }

    #[test]
    fn boxes_are_filled_when_lines_match() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        let picture = corpus.path().join("invoice.png");
        image::RgbImage::new(80, 40).save(&picture).unwrap();
        let file_id = index_without_boxes(
            &store,
            corpus.path(),
            &picture,
            "invoice.png\ninvoice total\nthank you",
        );

        let ocr = Fixed::new(vec![
            OcrLine {
                text: "invoice total".into(),
                bounds: boxed(0.1, 0.2, 0.6, 0.3),
            },
            OcrLine {
                text: "thank you".into(),
                bounds: boxed(0.1, 0.4, 0.5, 0.5),
            },
        ]);
        let summary = backfill(&store, &ocr).unwrap();
        assert_eq!((summary.boxed, summary.failed), (1, 0));
        assert_eq!(ocr.calls(), 1);

        let boxes = boxes_of(&store, file_id);
        assert_eq!(boxes.len(), 3, "the filename line plus two body lines");
        assert_eq!(boxes[0], None, "the filename has no place on the picture");
        assert_eq!(boxes[1], boxed(0.1, 0.2, 0.6, 0.3));
        assert_eq!(boxes[2], boxed(0.1, 0.4, 0.5, 0.5));

        // The row is no longer pending: a second run does not read the picture
        // again, and does not ask the engine a second time.
        let again = backfill(&store, &ocr).unwrap();
        assert_eq!(again.boxed, 0);
        assert_eq!(ocr.calls(), 1);
    }

    #[test]
    fn a_mismatch_stores_no_boxes_and_does_not_retry() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        let picture = corpus.path().join("scan.png");
        image::RgbImage::new(80, 40).save(&picture).unwrap();
        let file_id =
            index_without_boxes(&store, corpus.path(), &picture, "scan.png\ninvoice total");

        let ocr = Fixed::new(vec![OcrLine {
            text: "something else".into(),
            bounds: boxed(0.1, 0.2, 0.6, 0.3),
        }]);
        let summary = backfill(&store, &ocr).unwrap();
        assert_eq!((summary.boxed, summary.failed), (1, 0));
        assert!(boxes_of(&store, file_id).is_empty());

        // "No boxes" is an answer, not a failure: the next launch must not
        // read the same picture again hoping for a better one.
        let again = backfill(&store, &ocr).unwrap();
        assert_eq!(again.boxed, 0);
        assert_eq!(ocr.calls(), 1);
    }

    #[test]
    fn a_name_only_picture_is_not_read_again() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        let picture = corpus.path().join("dog.png");
        image::RgbImage::new(80, 40).save(&picture).unwrap();
        let file_id = index_without_boxes(&store, corpus.path(), &picture, "dog.png");

        let ocr = Fixed::new(vec![]);
        let summary = backfill(&store, &ocr).unwrap();
        assert_eq!((summary.boxed, summary.failed), (1, 0));
        assert_eq!(
            ocr.calls(),
            0,
            "a picture whose only text is its name has nothing to re-read"
        );
        assert!(boxes_of(&store, file_id).is_empty());

        let again = backfill(&store, &ocr).unwrap();
        assert_eq!(again.boxed, 0);
        assert_eq!(ocr.calls(), 0);
    }
}
