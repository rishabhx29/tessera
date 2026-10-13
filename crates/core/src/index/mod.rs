//! Format pipelines: turn one file on disk into searchable tiles.
//!
//! Every format (text today; images, PDF, DOCX, video later) lives behind the
//! [`Pipeline`] trait so a new format never touches the store or the search
//! code (AGENTS.md convention). A **tile** is the unit of indexing: one chunk
//! of a document, or the whole extracted text of an image.
//!
//! The run around the pipelines is incremental and honest: metadata decides
//! whether a file needs work at all, a file that must not be read is skipped
//! *before* anything opens it, a rename updates one row instead of indexing
//! the same bytes twice, and a deletion removes its results.

pub mod skip;
pub mod text;

use std::path::Path;

use anyhow::Result;

use crate::hash::LazyHash;
use crate::index::skip::{FileFacts, FileProbe, SystemProbe};
use crate::store::{FileState, Store};

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
    /// Files extracted: new, or changed since last time.
    pub indexed: usize,
    /// Files verified unchanged. Verified by a stat, not by re-reading them.
    pub unchanged: usize,
    /// Files that moved or were renamed: the row followed the file, and
    /// nothing was extracted.
    pub moved: usize,
    /// Files deliberately not read (cloud placeholder, oversize), each with
    /// its reason recorded on the row.
    pub skipped: usize,
    /// Files a pipeline accepted but could not extract; recorded on the file row.
    pub failed: usize,
    /// Rows dropped because the file is gone from disk.
    pub removed: usize,
    /// Paths that could not be walked or examined at all. Not file failures,
    /// but never silently dropped either — a folder that can't be read is
    /// exactly the "search can't find my file" case.
    pub walk_errors: usize,
}

/// Whether a file has to be re-extracted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Byte-identical to what is indexed.
    Unchanged,
    /// New or changed: extract it.
    Extract,
}

/// Decide whether a file on disk needs re-extracting.
///
/// Cheap evidence first. A file nobody touched still has its size and mtime,
/// and comparing two integers costs a stat, not a read — so a re-index over a
/// 20,000-file corpus reads nothing. Any movement in the timestamp — forward
/// or backward — sends the file to the content hash instead, because a moved
/// timestamp with an unchanged size is exactly what an equal-length edit looks
/// like (`port=8080` → `port=9090`), and a restored archive looks like a
/// backwards one. That is why `hash` is a closure: it is called exactly when
/// the cheap answer is inconclusive, and never otherwise.
///
/// The case this cannot catch is a rewrite with identical size *and* an
/// identical mtime — deliberately reconstructing the evidence. Catching it
/// would mean reading every file on every launch, which is the thing being
/// avoided; a `meta` flag for "always hash" is the escape hatch if it ever
/// matters.
pub fn decide(
    stored: Option<&FileState>,
    facts: &FileFacts,
    hash: impl FnOnce() -> Result<String>,
) -> Result<Change> {
    // A row that was never extracted (skipped, or failed last time) is pending
    // work whatever its metadata says.
    let Some(stored) = stored.filter(|s| s.extracted) else {
        return Ok(Change::Extract);
    };
    if stored.size == facts.size && stored.mtime == facts.mtime {
        return Ok(Change::Unchanged);
    }
    let Some(want) = stored.sha1.as_deref() else {
        return Ok(Change::Extract);
    };
    Ok(if hash()? == want {
        Change::Unchanged
    } else {
        Change::Extract
    })
}

/// A path the filesystem says is not there.
///
/// Only `NotFound` counts. A path that cannot be read is not evidence of
/// deletion, and treating it as one would empty an index over a permissions
/// hiccup or an unplugged drive.
pub fn is_gone(path: &Path) -> bool {
    matches!(
        std::fs::symlink_metadata(path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound
    )
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

/// Swap the filename at the head of a tile for the one the file has now — the
/// inverse of [`with_filename`]. A move must not cost a re-extract, but the
/// file still has to be findable under the name it carries today.
pub fn retitled(text: &str, new_name: &str) -> String {
    match text.split_once('\n') {
        Some((_, body)) => format!("{new_name}\n{body}"),
        None => new_name.to_string(),
    }
}

/// Drop rows for files under `root` that are no longer on disk, so a deletion
/// propagates instead of leaving behind a result nobody can open.
///
/// Scoped to the folder that was just walked, so indexing one folder never
/// forgets another. Existence is checked directly rather than inferred from
/// what the walk returned, because a directory the user cannot read looks
/// exactly like an empty one.
pub fn remove_missing(store: &Store, root: &Path) -> Result<usize> {
    let mut removed = 0;
    for (id, path) in store.rows_under(root)? {
        if is_gone(Path::new(&path)) {
            store.remove_file(id)?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Index every file under `folder` that any pipeline accepts.
///
/// One bad file never stops the run: it is recorded with its reason and the
/// walk continues.
pub fn index_folder(
    store: &Store,
    pipelines: &[Box<dyn Pipeline>],
    folder: &Path,
) -> Result<IndexSummary> {
    index_folder_with(store, pipelines, folder, &SystemProbe)
}

/// [`index_folder`] against an injected [`FileProbe`]. The seam exists for one
/// reason: a cloud placeholder cannot be created on demand, and the promise
/// that such a file is never opened has to be testable, not asserted.
pub fn index_folder_with(
    store: &Store,
    pipelines: &[Box<dyn Pipeline>],
    folder: &Path,
    probe: &dyn FileProbe,
) -> Result<IndexSummary> {
    use walkdir::WalkDir;

    if !folder.is_dir() {
        anyhow::bail!("{} is not a folder", folder.display());
    }

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
        // Metadata only. Nothing below this line may open a file that
        // `skip_reason` rejects.
        let facts = match probe.facts(path) {
            Ok(facts) => facts,
            Err(_) => {
                summary.walk_errors += 1;
                continue;
            }
        };
        if let Some(reason) = self::skip::skip_reason(&facts) {
            store.skip_file(folder, path, pipeline.kind(), &facts, reason)?;
            summary.skipped += 1;
            continue;
        }
        let stored = store.file_state(path)?;
        let mut hash = LazyHash::new(path);
        if decide(stored.as_ref(), &facts, || Ok(hash.get()?.to_string()))? == Change::Unchanged {
            store.mark_unchanged(path, &facts)?;
            summary.unchanged += 1;
            continue;
        }
        // New or changed bytes. The hash above may already have read the file;
        // `LazyHash` makes sure that is the only time it does.
        if stored.is_none() {
            // An unknown path whose bytes match a row under the same folder
            // is that file, moved. Reclaiming the row keeps one file indexed
            // once, under the name it has now — and only when the old path is
            // really gone, so two live copies stay two files.
            let moved = store
                .moved_candidates(folder, hash.get()?, path)?
                .into_iter()
                .find(|(_, old)| is_gone(Path::new(old)));
            if let Some((id, _)) = moved {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();
                let first_tile = store.tile_texts(id)?.into_iter().next();
                let new_title = first_tile.map(|text| retitled(&text, &name));
                store.move_file(id, path, &facts, hash.get()?, new_title.as_deref())?;
                summary.moved += 1;
                continue;
            }
        }
        match pipeline.process(path) {
            Ok(tiles) => {
                store.index_file(
                    folder,
                    path,
                    pipeline.kind(),
                    &facts,
                    Some(hash.get()?),
                    &with_filename(path, tiles),
                )?;
                summary.indexed += 1;
            }
            Err(err) => {
                store.fail_file(folder, path, pipeline.kind(), &facts, &err.to_string())?;
                summary.failed += 1;
            }
        }
    }
    summary.removed = remove_missing(store, folder)?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(size: u64, mtime: i64) -> FileFacts {
        FileFacts {
            size,
            mtime,
            placeholder: false,
        }
    }

    fn state(size: u64, mtime: i64, sha1: Option<&str>, extracted: bool) -> FileState {
        FileState {
            size,
            mtime,
            sha1: sha1.map(str::to_string),
            extracted,
        }
    }

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

    #[test]
    fn retitling_swaps_the_name_and_keeps_the_text() {
        assert_eq!(
            retitled("a.txt\ninvoice airline", "b.txt"),
            "b.txt\ninvoice airline"
        );
        assert_eq!(
            retitled("a.txt", "b.txt"),
            "b.txt",
            "a textless tile is just the name"
        );
        assert_eq!(retitled("a.txt\n", "b.txt"), "b.txt\n");
    }

    #[test]
    fn an_untouched_file_is_verified_without_reading_it() {
        let stored = state(10, 100, Some("abc"), true);
        let mut read = false;
        let change = decide(Some(&stored), &facts(10, 100), || {
            read = true;
            Ok("abc".to_string())
        })
        .unwrap();
        assert_eq!(change, Change::Unchanged);
        assert!(
            !read,
            "a stat is enough; reading 20k files to re-index is not"
        );
    }

    #[test]
    fn a_touched_file_is_decided_by_its_hash() {
        let stored = state(10, 100, Some("abc"), true);
        let mut read = false;
        let change = decide(Some(&stored), &facts(10, 200), || {
            read = true;
            Ok("abc".to_string())
        })
        .unwrap();
        assert_eq!(
            change,
            Change::Unchanged,
            "same bytes, new mtime: nothing to do"
        );
        assert!(read, "inconclusive metadata must fall through to the hash");
    }

    #[test]
    fn changed_bytes_are_re_extracted() {
        let stored = state(10, 100, Some("abc"), true);
        let change = decide(Some(&stored), &facts(10, 200), || {
            Ok("different".to_string())
        })
        .unwrap();
        assert_eq!(change, Change::Extract);
    }

    #[test]
    fn a_clock_that_went_backwards_is_not_evidence_of_untouched() {
        // A restored archive keeps an old timestamp: same size, older mtime.
        // That looks untouched and is not, so it goes to the hash.
        let stored = state(10, 100, Some("abc"), true);
        let mut read = false;
        let change = decide(Some(&stored), &facts(10, 50), || {
            read = true;
            Ok("different".to_string())
        })
        .unwrap();
        assert_eq!(change, Change::Extract);
        assert!(read);
    }

    #[test]
    fn a_later_timestamp_with_the_same_size_is_still_hash_checked() {
        let stored = state(10, 100, Some("abc"), true);
        let mut read = false;
        let change = decide(Some(&stored), &facts(10, 300), || {
            read = true;
            Ok("abc".to_string())
        })
        .unwrap();
        assert_eq!(change, Change::Unchanged);
        assert!(
            read,
            "a moved timestamp with an unchanged size is an equal-length edit until proven otherwise"
        );
    }

    #[test]
    fn an_unknown_path_is_extracted_whatever_the_hash_says() {
        let change = decide(None, &facts(10, 100), || Ok("abc".to_string())).unwrap();
        assert_eq!(change, Change::Extract);
    }

    #[test]
    fn a_row_whose_content_was_never_extracted_is_pending_work() {
        let stored = state(10, 100, Some("abc"), false);
        let mut read = false;
        let change = decide(Some(&stored), &facts(10, 100), || {
            read = true;
            Ok("abc".to_string())
        })
        .unwrap();
        assert_eq!(change, Change::Extract);
        assert!(
            !read,
            "matching metadata is not evidence that it was indexed"
        );
    }

    #[test]
    fn a_row_without_a_hash_is_re_extracted_rather_than_guessed_at() {
        let stored = state(10, 100, None, true);
        let change = decide(Some(&stored), &facts(10, 200), || {
            panic!("nothing to compare against")
        })
        .unwrap();
        assert_eq!(change, Change::Extract);
    }

    #[test]
    fn a_path_that_is_gone_and_one_that_is_unreadable_are_different_things() {
        let dir = tempfile::tempdir().unwrap();
        let present = dir.path().join("here.txt");
        std::fs::write(&present, "x").unwrap();
        assert!(!is_gone(&present));
        assert!(is_gone(&dir.path().join("gone.txt")));
    }
}
