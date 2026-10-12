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

/// Index every file under `folder` that any pipeline accepts.
///
/// Returns `(indexed, failed)` counts. One bad file never stops the run; it
/// is recorded with its reason and the walk continues.
pub fn index_folder(
    store: &crate::store::Store,
    pipelines: &[Box<dyn Pipeline>],
    folder: &Path,
) -> anyhow::Result<(usize, usize)> {
    use walkdir::WalkDir;

    let mut indexed = 0usize;
    let mut failed = 0usize;
    for entry in WalkDir::new(folder)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(pipeline) = pipelines.iter().find(|p| p.accepts(path)) else {
            continue;
        };
        match pipeline.process(path) {
            Ok(tiles) => {
                store.index_file(path, pipeline.kind(), &tiles)?;
                indexed += 1;
            }
            Err(err) => {
                store.fail_file(path, pipeline.kind(), &err.to_string())?;
                failed += 1;
            }
        }
    }
    Ok((indexed, failed))
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
}
