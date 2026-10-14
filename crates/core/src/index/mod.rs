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

pub mod image;
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

/// What kind of file a pipeline produces. A closed set, so a format added
/// later is a compile error everywhere it must be handled — the store column,
/// the search result, the UI's type badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
    Pdf,
    Docx,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Image => "image",
            Kind::Pdf => "pdf",
            Kind::Docx => "docx",
        }
    }
}

/// A format pipeline: accepts some files, turns each into tiles.
///
/// One method decides both *whether* a pipeline owns a file and *what kind* it
/// is, because those two answers cannot usefully disagree: a pipeline that
/// accepts a file and then stamps it with the wrong kind would mislabel its
/// row, and a file whose kind is guessed from the extension after the fact is a
/// guess.
pub trait Pipeline {
    /// The kind this pipeline produces for `path`, or `None` if it does not
    /// own this file.
    fn accepts(&self, path: &Path) -> Option<Kind>;

    /// Extract the file's content into tiles. Errors mean "this file failed",
    /// never "the whole index run failed".
    fn process(&self, path: &Path) -> anyhow::Result<Vec<Tile>>;

    /// The file's embedding for the semantic arm, if this pipeline produces
    /// one.
    ///
    /// Separate from [`Pipeline::process`] because the two are different work
    /// with different failure modes and different costs: text extraction is
    /// microseconds and always succeeds for a readable file, while embedding a
    /// SigLIP image is ~200 ms and needs 213 MB of models that may not be
    /// installed. Keeping them apart means a folder of documents does not pay
    /// to load a vision tower, and a missing model set degrades the arm rather
    /// than failing the file.
    ///
    /// `Ok(None)` means "this pipeline has nothing to say about this file",
    /// which is the right answer for text formats and for any pipeline whose
    /// model set is not installed.
    fn embed(&self, _path: &Path) -> anyhow::Result<Option<crate::vectors::Embedding>> {
        Ok(None)
    }
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
    /// Files that were indexed but could not be embedded for description
    /// search. Separate from `failed` because the text landed: these files are
    /// fully findable by keywords, and only the description arm is missing. The
    /// count exists so "why can't I search my photos by description?" is
    /// answerable from `tessera stats` rather than by guessing.
    pub unembedded: usize,
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
///
/// Paired with [`with_filename`] and called by the store when it re-identifies
/// a file at a new path: the filename convention is written on one side and
/// read on the other, never reconstructed by a caller.
pub fn retitled(text: &str, new_name: &str) -> String {
    match text.split_once('\n') {
        Some((_, body)) => format!("{new_name}\n{body}"),
        None => new_name.to_string(),
    }
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

/// [`index_folder`] against an injected [`FileProbe`]. Internal: the probe is
/// a test seam with one production adapter, so it does not belong on the public
/// interface — callers get the filesystem probe and nothing to choose.
pub(crate) fn index_folder_with(
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
        let Some((pipeline, kind)) = pipelines
            .iter()
            .find_map(|p| p.accepts(path).map(|kind| (p, kind)))
        else {
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
            store.skip_file(folder, path, kind, &facts, reason)?;
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
            // An unknown path whose bytes match a row under the same folder is
            // that file, moved. Reclaiming the row keeps one file indexed
            // once, under the name it has now; the store decides whether the
            // old path is really gone, so two live copies stay two files.
            if let Some((id, _)) = store.moved_candidate(folder, hash.get()?, path)? {
                store.move_file(id, path, &facts, hash.get()?)?;
                summary.moved += 1;
                continue;
            }
        }
        // The text arm and the semantic arm are separate capabilities that happen to
        // share a file, so they are recorded separately. Embedding is attempted
        // even when OCR failed: a machine with no OCR language pack can still
        // find its photographs by description, and losing the whole image
        // because the *text* half failed would be throwing away a working half.
        let row_written = match pipeline.process(path) {
            Ok(tiles) => {
                let sha = Some(hash.get()?);
                store.index_file(folder, path, kind, &facts, sha, &with_filename(path, tiles))?;
                summary.indexed += 1;
                true
            }
            Err(err) => {
                // `{err:#}` walks the whole context chain. `to_string()` prints
                // only the outermost context, which for the OCR adapters is
                // "decoding <path>" — a reason that names the file and nothing
                // else. The recorded reason is the only thing the user has to go
                // on (SECURITY.md: skips are recorded, never swallowed), so the
                // cause has to be in it.
                store.fail_file(folder, path, kind, &facts, &format!("{err:#}"))?;
                summary.failed += 1;
                true
            }
        };

        // After the row exists, so a vector can be attached to it either way.
        if row_written {
            match pipeline.embed(path) {
                Ok(Some(embedding)) => match store.file_id(path) {
                    Ok(file_id) => store.put_image_embedding(file_id, &embedding)?,
                    Err(err) => eprintln!(
                        "recorded {} but could not find its row to attach an embedding: {err:#}",
                        path.display()
                    ),
                },
                // No embedder, or nothing to say about this file.
                Ok(None) => {}
                Err(err) => {
                    eprintln!(
                        "could not embed {} for description search: {err:#}",
                        path.display()
                    );
                    summary.unembedded += 1;
                }
            }
        }
    }
    summary.removed = store.remove_missing(folder)?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::text::TextPipeline;
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
            kind: "text".to_string(),
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

    // -- The skip rules, through the index loop ------------------------
    //
    // These live here, not in tests/incremental.rs, because they need the
    // probe seam — which is internal, and deliberately so.

    /// Reports files whose names contain `marker` as cloud placeholders: the
    /// Files On-Demand state a real OneDrive folder produces and no test can
    /// otherwise create on demand.
    struct CloudyProbe {
        marker: &'static str,
    }

    impl FileProbe for CloudyProbe {
        fn facts(&self, path: &Path) -> Result<FileFacts> {
            let mut facts = SystemProbe.facts(path)?;
            if path.to_string_lossy().contains(self.marker) {
                facts.placeholder = true;
            }
            Ok(facts)
        }
    }

    fn write(dir: &Path, name: &str, contents: &str) {
        std::fs::write(dir.join(name), contents).unwrap();
    }

    #[test]
    fn a_placeholder_is_skipped_and_the_pipeline_never_sees_it() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let dir = corpus.path();
        write(dir, "local.txt", "the invoice from the airline");
        write(dir, "cloud-only.txt", "the invoice from the cloud");
        let store = Store::open(data.path()).unwrap();

        // A pipeline that panics if it is ever asked to read the placeholder.
        // The skip has to happen before the pipeline, or this fails instead of
        // passing vacuously.
        struct ExplodesOnPlaceholder;
        impl Pipeline for ExplodesOnPlaceholder {
            fn accepts(&self, path: &Path) -> Option<Kind> {
                TextPipeline.accepts(path)
            }
            fn process(&self, path: &Path) -> Result<Vec<Tile>> {
                assert!(
                    !path.to_string_lossy().contains("cloud-only"),
                    "the pipeline was asked to open the cloud placeholder {}",
                    path.display()
                );
                TextPipeline.process(path)
            }
        }

        let summary = index_folder_with(
            &store,
            &[Box::new(ExplodesOnPlaceholder)],
            dir,
            &CloudyProbe {
                marker: "cloud-only",
            },
        )
        .unwrap();

        assert_eq!(
            (summary.indexed, summary.skipped, summary.failed),
            (1, 1, 0)
        );
        // Recorded with its reason, and its bytes never became searchable text.
        let row = store
            .file_state(&dir.join("cloud-only.txt"))
            .unwrap()
            .expect("the skipped file is recorded");
        assert!(!row.extracted);
        assert_eq!(
            store.skip_reasons().unwrap(),
            vec![("cloud_placeholder".to_string(), 1)]
        );
        assert!(
            store
                .file_ids_matching(&crate::search::term_match_expr("cloud"))
                .unwrap()
                .is_empty(),
            "a file that was never read must not have searchable text"
        );
    }

    #[test]
    fn the_diagnostics_break_down_why_files_were_not_read() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let dir = corpus.path();
        write(dir, "ok.txt", "the invoice from the airline");
        write(dir, "cloud.txt", "placeholder");
        let huge = std::fs::File::create(dir.join("huge.txt")).unwrap();
        huge.set_len(skip::MAX_FILE_BYTES + 1).unwrap();
        drop(huge);
        // Extraction that fails: recorded as failed, and kept out of the
        // reason breakdown, because its reason is an error message.
        std::fs::write(dir.join("bad.txt"), [0xFF, 0xFE, 0x00, 0xD8]).unwrap();
        let store = Store::open(data.path()).unwrap();

        index_folder_with(
            &store,
            &[Box::new(TextPipeline)],
            dir,
            &CloudyProbe {
                marker: "cloud.txt",
            },
        )
        .unwrap();

        assert_eq!(
            store.stats().unwrap(),
            vec![
                ("failed".to_string(), 1),
                ("indexed".to_string(), 1),
                ("skipped".to_string(), 2),
            ]
        );
        assert_eq!(
            store.skip_reasons().unwrap(),
            vec![
                ("cloud_placeholder".to_string(), 1),
                ("too_large".to_string(), 1)
            ]
        );
    }

    #[test]
    fn a_file_one_drive_evicted_stays_findable_and_is_still_recorded() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let dir = corpus.path();
        write(dir, "invoice.txt", "the invoice from the airline arrived");
        let store = Store::open(data.path()).unwrap();
        index_folder(&store, &[Box::new(TextPipeline)], dir).unwrap();

        // OneDrive evicts the file under disk pressure: the bytes leave, the
        // row does not. Searching the text we already hold downloads nothing,
        // and dropping it would remove a result for a reason the user neither
        // caused nor can undo.
        let summary = index_folder_with(
            &store,
            &[Box::new(TextPipeline)],
            dir,
            &CloudyProbe {
                marker: "invoice.txt",
            },
        )
        .unwrap();
        assert_eq!((summary.skipped, summary.indexed), (1, 0));
        assert_eq!(
            store.skip_reasons().unwrap(),
            vec![("cloud_placeholder".to_string(), 1)]
        );
        assert_eq!(
            store
                .files_by_ids(
                    &store
                        .file_ids_matching(&crate::search::term_match_expr("airline"))
                        .unwrap()
                        .into_iter()
                        .collect::<Vec<_>>()
                )
                .unwrap()
                .len(),
            1,
            "the text of an evicted file is no longer searchable"
        );
    }
}
