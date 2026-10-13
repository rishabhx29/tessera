//! Format pipelines: turn one file on disk into searchable tiles.
//!
//! Every format (text today; images, PDF, DOCX, video later) lives behind the
//! [`Pipeline`] trait so a new format never touches the store or the search
//! code (AGENTS.md convention). A **tile** is the unit of indexing: one chunk
//! of a document, or the whole extracted text of an image.

pub mod text;

use std::path::Path;

/// One indexed unit: a numbered chunk of extracted text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    pub seq: u32,
    pub text: String,
}

/// A format pipeline: accepts some files, turns each into tiles.
pub trait Pipeline {
    /// Whether this pipeline handles the given path (by extension).
    fn accepts(&self, path: &Path) -> bool;

    /// Extract the file's content into tiles. Errors mean "this file failed",
    /// never "the whole index run failed".
    fn process(&self, path: &Path) -> anyhow::Result<Vec<Tile>>;

    /// The `kind` recorded on the file row ("text", "image", "pdf", ...).
    fn kind(&self) -> &'static str;
}

/// What one `index_folder` run did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IndexSummary {
    /// Files indexed (or re-indexed) successfully.
    pub indexed: usize,
    /// Files a pipeline accepted but could not extract; recorded on the file row.
    pub failed: usize,
    /// Directories the walker could not read. Not file failures, but never
    /// silently dropped either — a folder that can't be walked is exactly the
    /// "search can't find my file" case.
    pub walk_errors: usize,
}

/// Make a file's name part of its searchable text: prepended to the first
/// tile, or a lone tile when the file extracted no text. A name is one of the
/// three things people search for (product-definition 3.1).
fn with_filename(path: &Path, mut tiles: Vec<Tile>) -> Vec<Tile> {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return tiles;
    };
    match tiles.first_mut() {
        Some(first) => first.text = format!("{name}\n{}", first.text),
        None => tiles.push(Tile {
            seq: 0,
            text: name.to_string(),
        }),
    }
    tiles
}

/// Index every file under `folder` that any pipeline accepts.
///
/// One bad file never stops the run: it is recorded with its reason and the
/// walk continues.
pub fn index_folder(
    store: &crate::store::Store,
    pipelines: &[Box<dyn Pipeline>],
    folder: &Path,
) -> anyhow::Result<IndexSummary> {
    use walkdir::WalkDir;

    let mut summary = IndexSummary::default();
    for entry in WalkDir::new(folder).follow_links(false).into_iter() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                summary.walk_errors += 1;
                continue;
            }
        };
        let path = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(pipeline) = pipelines.iter().find(|p| p.accepts(path)) else {
            continue;
        };
        match pipeline.process(path) {
            Ok(tiles) => {
                store.index_file(path, pipeline.kind(), &with_filename(path, tiles))?;
                summary.indexed += 1;
            }
            Err(err) => {
                store.fail_file(path, pipeline.kind(), &err.to_string())?;
                summary.failed += 1;
            }
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_holds_seq_and_text() {
        let t = Tile {
            seq: 0,
            text: "hello".into(),
        };
        assert_eq!(t.seq, 0);
        assert_eq!(t.text, "hello");
        let _ = t; // Tile is Clone + Debug + PartialEq, exercised by the derive
    }

    #[test]
    fn filename_leads_the_first_tile() {
        let tiles = with_filename(
            Path::new("notes/zwiebel.md"),
            vec![Tile {
                seq: 0,
                text: "body".into(),
            }],
        );
        assert_eq!(tiles[0].text, "zwiebel.md\nbody");
    }

    #[test]
    fn a_textless_file_is_still_findable_by_name() {
        let tiles = with_filename(Path::new("empty/photo.png.txt"), Vec::new());
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].text, "photo.png.txt");
    }
}
