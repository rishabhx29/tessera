//! Format pipelines: turn one file on disk into searchable tiles.
//!
//! Every format (text, images, PDF, DOCX today; video later) lives behind the
//! [`Pipeline`] trait so a new format never touches the store or the search
//! code (AGENTS.md convention). A **tile** is the unit of indexing: one chunk
//! of a document, or the whole extracted text of an image.
//!
//! The run around the pipelines is incremental and honest: metadata decides
//! whether a file needs work at all, a file that must not be read is skipped
//! *before* anything opens it, a rename updates one row instead of indexing
//! the same bytes twice, and a deletion removes its results.

pub mod docx;
pub mod image;
pub mod pdf;
pub mod skip;
pub mod text;

use std::path::Path;

use anyhow::Result;

use crate::hash::LazyHash;
use crate::index::skip::{FileFacts, FileProbe, SystemProbe};
use crate::store::{Arm, FileState, Store};
use crate::vectors::TextEmbedder;

/// One indexed unit: a numbered chunk of extracted text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    pub seq: u32,
    pub text: String,
    /// The page this tile came from, for formats that have pages (PDF).
    /// 1-based, because it is what a person reads off the page ("page 4");
    /// `None` for formats without them.
    pub page: Option<u32>,
}

/// What processing one file produced.
///
/// Three outcomes, not two, because "this file could not be read" and "there
/// is nothing to read *yet*" are different facts about it: a scanned PDF has
/// no text layer, and filing it as failed would blame the user's file for a
/// capability this build does not have. What each outcome becomes in the
/// store is the loop's business; the pipeline only says which one happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extraction {
    /// The file's searchable content — possibly empty, because a file with no
    /// text at all is still findable by its name.
    Tiles(Vec<Tile>),
    /// The file is images rather than text: a PDF with no text layer, waiting
    /// for the OCR pass the spec schedules for v0.1.x. Recorded as
    /// `awaiting_ocr` with its reason — not a failure, and never silently
    /// dropped ("SKIP reasons are recorded, not swallowed", architecture).
    AwaitingOcr,
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

/// The kind of a path, from the engine's format table, without building a
/// pipeline.
///
/// Queue admission needs to know whether a path is worth enqueuing before any
/// model is loaded — a watcher sees events for every file in a folder, and
/// queueing a `.zip` only to skip it later is work for nothing. This is the
/// same table [`Pipeline::accepts`] implements (text, PDF, DOCX, then image);
/// a test holds the two together for every extension, so the helper cannot
/// drift into admitting a format no pipeline owns.
pub fn kind_for(path: &Path) -> Option<Kind> {
    if let Some(kind) = text::TextPipeline.accepts(path) {
        return Some(kind);
    }
    if let Some(kind) = pdf::PdfPipeline.accepts(path) {
        return Some(kind);
    }
    if let Some(kind) = docx::DocxPipeline.accepts(path) {
        return Some(kind);
    }
    let extension = path.extension()?.to_str()?;
    image::claims_extension(extension).then_some(Kind::Image)
}

/// A format pipeline: accepts some files, turns each into tiles.
///
/// One method decides both *whether* a pipeline owns a file and *what kind* it
/// is, because those two answers cannot usefully disagree: a pipeline that
/// accepts a file and then stamps it with the wrong kind would mislabel its
/// row, and a file whose kind is guessed from the extension after the fact is a
/// guess.
pub trait Pipeline: Send + Sync {
    /// The kind this pipeline produces for `path`, or `None` if it does not
    /// own this file.
    fn accepts(&self, path: &Path) -> Option<Kind>;

    /// Extract the file's content. Errors mean "this file failed", never "the
    /// whole index run failed"; [`Extraction::AwaitingOcr`] means the file has
    /// no text this build can read.
    fn process(&self, path: &Path) -> anyhow::Result<Extraction>;

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
    /// Files whose content is images rather than text (scanned PDFs): recorded
    /// as awaiting an OCR pass, counted separately so "why can't I search my
    /// scans?" has an answer that is not "failure".
    pub awaiting_ocr: usize,
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
    /// Tiles this run embedded for the text arm. Tiles, not files: a PDF with
    /// forty pages and a text file with one chunk are both one file and very
    /// different amounts of work.
    pub text_embedded: usize,
    /// Tiles the text arm could not embed. Like `unembedded`, the text is still
    /// searchable by keyword; only meaning search lacks this tile.
    pub text_unembedded: usize,
}

impl IndexSummary {
    /// Fold another run's counters into this one.
    ///
    /// The watcher gets one summary per file from a worker and needs a total;
    /// summing field by field at the call site is how a new counter gets
    /// forgotten in one of the two places.
    pub(crate) fn absorb(&mut self, other: &IndexSummary) {
        self.indexed += other.indexed;
        self.unchanged += other.unchanged;
        self.moved += other.moved;
        self.skipped += other.skipped;
        self.failed += other.failed;
        self.awaiting_ocr += other.awaiting_ocr;
        self.removed += other.removed;
        self.walk_errors += other.walk_errors;
        self.unembedded += other.unembedded;
        self.text_embedded += other.text_embedded;
        self.text_unembedded += other.text_unembedded;
    }
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

/// Whether a file is worth queueing at all, from metadata alone.
///
/// [`decide`] answers "must these bytes be read?" and may fall through to a
/// hash. This answers the cheaper question the watcher asks on every event and
/// every 60-second rescan — "is there any chance this needs work?" — without
/// ever reading a byte: a path with no row, a row that was never extracted, or
/// metadata that moved. A false positive costs one hash inside `process_one`
/// (which then verifies it unchanged); a false negative would leave a file
/// stale, so the answer leans towards "yes".
pub(crate) fn needs_work(stored: Option<&FileState>, facts: &FileFacts) -> bool {
    match stored {
        None => true,
        Some(state) => !state.extracted || state.size != facts.size || state.mtime != facts.mtime,
    }
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
            page: None,
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

/// Process one file: the whole per-file pass, shared by the folder walk and the
/// watcher.
///
/// The decisions are in this order and the order is the contract:
/// skip rules first (metadata only, so a placeholder is never opened), then the
/// cheap change decision, then the move question for an unknown path, then
/// extraction, then the image embedding. Counters are updated exactly here, so
/// `tessera index` and `tessera watch` cannot disagree about what a skipped or
/// unchanged file is.
///
/// `facts` are the metadata snapshot taken when the file was admitted to the
/// walk or the queue. A writer that changes the file after that snapshot is
/// caught by the next event or by the watcher's safety rescan, which re-stats
/// and either re-extracts (bytes differ) or repairs the metadata (same bytes).
pub(crate) fn process_one(
    store: &Store,
    folder: &Path,
    path: &Path,
    pipeline: &dyn Pipeline,
    kind: Kind,
    facts: &FileFacts,
    summary: &mut IndexSummary,
) -> Result<()> {
    // Metadata only. Nothing below this line may open a file that
    // `skip_reason` rejects.
    if let Some(reason) = self::skip::skip_reason(facts) {
        store.skip_file(folder, path, kind, facts, reason)?;
        summary.skipped += 1;
        return Ok(());
    }
    let stored = store.file_state(path)?;
    let mut hash = LazyHash::new(path);
    if decide(stored.as_ref(), facts, || Ok(hash.get()?.to_string()))? == Change::Unchanged {
        store.mark_unchanged(path, facts)?;
        summary.unchanged += 1;
        return Ok(());
    }
    // New or changed bytes. The hash above may already have read the file;
    // `LazyHash` makes sure that is the only time it does.
    if stored.is_none() {
        // An unknown path whose bytes match a row under the same folder is
        // that file, moved. Reclaiming the row keeps one file indexed
        // once, under the name it has now; the store decides whether the
        // old path is really gone, so two live copies stay two files.
        if let Some((id, _)) = store.moved_candidate(folder, hash.get()?, path)? {
            store.move_file(id, path, facts, hash.get()?)?;
            summary.moved += 1;
            return Ok(());
        }
    }
    // The text arm and the semantic arm are separate capabilities that happen to
    // share a file, so they are recorded separately. Embedding is attempted
    // even when OCR failed: a machine with no OCR language pack can still
    // find its photographs by description, and losing the whole image
    // because the *text* half failed would be throwing away a working half.
    let row_written = match pipeline.process(path) {
        Ok(Extraction::Tiles(tiles)) => {
            let sha = Some(hash.get()?);
            store.index_file(folder, path, kind, facts, sha, &with_filename(path, tiles))?;
            summary.indexed += 1;
            true
        }
        Ok(Extraction::AwaitingOcr) => {
            store.await_ocr(folder, path, kind, facts)?;
            summary.awaiting_ocr += 1;
            true
        }
        Err(err) => {
            // `{err:#}` walks the whole context chain. `to_string()` prints
            // only the outermost context, which for the OCR adapters is
            // "decoding <path>" — a reason that names the file and nothing
            // else. The recorded reason is the only thing the user has to go
            // on (SECURITY.md: skips are recorded, never swallowed), so the
            // cause has to be in it.
            store.fail_file(folder, path, kind, facts, &format!("{err:#}"))?;
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
    Ok(())
}

/// The text arm's whole write path: every tile with no MiniLM vector gets one.
///
/// A sweep over the store rather than work inside the per-file pass, because
/// the repair path and the fresh-install path are the same code. Returns
/// `(embedded, unembedded)`; the caller decides whether that is enough to claim
/// the arm's version.
pub(crate) fn sweep_text(store: &Store, text: &dyn TextEmbedder) -> Result<(usize, usize)> {
    let mut embedded = 0;
    let mut unembedded = 0;
    for (tile_id, tile_text) in store.tiles_without_text_vectors()? {
        match text.embed_text(&tile_text) {
            Ok(embedding) => {
                store.put_text_embedding(tile_id, &embedding)?;
                embedded += 1;
            }
            Err(err) => {
                // One unembeddable tile must not end the sweep or the run.
                // It stays in the work list for the next pass.
                eprintln!("could not embed tile {tile_id} for meaning search: {err:#}");
                unembedded += 1;
            }
        }
    }
    Ok((embedded, unembedded))
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
    index_folder_with(store, pipelines, folder, &SystemProbe, None)
}

/// [`index_folder`] with the text arm attached.
///
/// After the walk, every tile with no MiniLM vector gets one — including tiles
/// indexed by an earlier run before the models were installed, and the whole
/// index after a model pin moved. That is why this is a sweep over the store
/// rather than work done inside the per-file loop: the repair path and the
/// fresh-install path are the same code.
pub fn index_folder_with_text(
    store: &Store,
    pipelines: &[Box<dyn Pipeline>],
    folder: &Path,
    text: &dyn TextEmbedder,
) -> Result<IndexSummary> {
    index_folder_with(store, pipelines, folder, &SystemProbe, Some(text))
}

/// [`index_folder`] against an injected [`FileProbe`]. Internal: the probe is
/// a test seam with one production adapter, so it does not belong on the public
/// interface — callers get the filesystem probe and nothing to choose.
pub(crate) fn index_folder_with(
    store: &Store,
    pipelines: &[Box<dyn Pipeline>],
    folder: &Path,
    probe: &dyn FileProbe,
    text: Option<&dyn TextEmbedder>,
) -> Result<IndexSummary> {
    use walkdir::WalkDir;

    if !folder.is_dir() {
        anyhow::bail!("{} is not a folder", folder.display());
    }

    // Vectors from two MiniLM builds are not comparable. The CLI drops the arm
    // when the pin moves; a caller that did not is refused rather than allowed
    // to blend two spaces in one index.
    if let Some(text) = text {
        if let Some(recorded) = store.arm_version(Arm::Text)? {
            if recorded != text.version() {
                anyhow::bail!(
                    "the text arm's vectors were built by MiniLM {recorded}, but the pinned model \
                     is {}; drop the arm with `Store::forget_text_arm` and re-index instead of \
                     mixing two vector spaces",
                    text.version()
                );
            }
        }
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
        let facts = match probe.facts(path) {
            Ok(facts) => facts,
            Err(_) => {
                summary.walk_errors += 1;
                continue;
            }
        };
        process_one(
            store,
            folder,
            path,
            pipeline.as_ref(),
            kind,
            &facts,
            &mut summary,
        )?;
    }
    summary.removed = store.remove_missing(folder)?;

    // The text arm's whole write path. It runs last so a file removed above
    // cannot leave a vector behind, and it reads the tiles the store already
    // holds, so re-embedding never re-reads a file.
    if let Some(text) = text {
        let (embedded, unembedded) = sweep_text(store, text)?;
        summary.text_embedded += embedded;
        summary.text_unembedded += unembedded;
        // Only claim the arm is built by this model when every tile in the
        // index has a vector from it. A partial run is outstanding work, which
        // is exactly what the missing pin means.
        if summary.text_unembedded == 0 {
            store.set_arm_version(Arm::Text, text.version())?;
        }
    }

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

    /// `kind_for` is the watcher's admission table and the workers find a
    /// pipeline by asking the same table: if the two disagree, the watcher
    /// queues a file no worker accepts and that file never gets a row. Every
    /// extension either side knows is checked, plus the case-folding and the
    /// plain-junk cases a real folder contains.
    #[test]
    fn queue_admission_agrees_with_the_pipeline_table() {
        let pipelines: Vec<Box<dyn Pipeline>> = vec![
            Box::new(TextPipeline),
            Box::new(pdf::PdfPipeline),
            Box::new(docx::DocxPipeline),
            Box::new(image::ImagePipeline::new(Box::new(
                crate::ocr::Unavailable("test"),
            ))),
        ];
        for name in [
            "a.txt", "a.md", "a.csv", "a.log", "a.pdf", "a.docx", "a.png", "a.jpg", "a.jpeg",
            "a.webp", "a.bmp", "a.gif", "a.tiff", "a.zip", "a.tar.gz", "README", "a.PDF", "a.Txt",
        ] {
            let path = Path::new(name);
            let by_table = pipelines.iter().find_map(|pipeline| pipeline.accepts(path));
            assert_eq!(kind_for(path), by_table, "admission disagrees on {name}");
        }
        // And the table is not empty: a test where every answer was `None`
        // would pass while the watcher queued nothing at all.
        assert_eq!(kind_for(Path::new("a.txt")), Some(Kind::Text));
        assert_eq!(kind_for(Path::new("a.png")), Some(Kind::Image));
    }

    #[test]
    fn tile_holds_seq_text_and_page() {
        let t = Tile {
            seq: 0,
            text: "hello".into(),
            page: Some(4),
        };
        assert_eq!(t.seq, 0);
        assert_eq!(t.text, "hello");
        assert_eq!(t.page, Some(4), "a tile from a paged format knows its page");
        let _ = t; // Tile is Clone + Debug + PartialEq, exercised by the derive
    }

    #[test]
    fn filename_leads_the_first_tile() {
        let tiles = with_filename(
            Path::new("notes/zwiebel.md"),
            vec![Tile {
                seq: 0,
                text: "body".into(),
                page: Some(2),
            }],
        );
        assert_eq!(tiles[0].text, "zwiebel.md\nbody");
        assert_eq!(
            tiles[0].page,
            Some(2),
            "prepending the name must not move the tile off its page"
        );
    }

    #[test]
    fn a_textless_file_is_still_findable_by_name() {
        let tiles = with_filename(Path::new("empty/photo.png.txt"), Vec::new());
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].text, "photo.png.txt");
        assert_eq!(tiles[0].page, None, "a name tile belongs to no page");
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
            fn process(&self, path: &Path) -> Result<Extraction> {
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
            None,
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

    /// A pipeline that claims `.scan` files and reports that nothing can be
    /// read yet — the shape a PDF with no text layer produces.
    struct ScansNothing;

    impl Pipeline for ScansNothing {
        fn accepts(&self, path: &Path) -> Option<Kind> {
            (path.extension().and_then(|e| e.to_str())? == "scan").then_some(Kind::Pdf)
        }

        fn process(&self, _path: &Path) -> Result<Extraction> {
            Ok(Extraction::AwaitingOcr)
        }
    }

    #[test]
    fn a_file_with_nothing_to_read_yet_is_awaiting_ocr_not_failed() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let dir = corpus.path();
        write(dir, "invoice.scan", "bytes whose content is an image");

        let store = Store::open(data.path()).unwrap();
        let summary = index_folder(&store, &[Box::new(ScansNothing)], dir).unwrap();

        assert_eq!(
            (summary.indexed, summary.failed, summary.awaiting_ocr),
            (0, 0, 1),
            "waiting for OCR is its own outcome, not a failure and not an index"
        );
        assert_eq!(
            store.stats().unwrap(),
            vec![("awaiting_ocr".to_string(), 1)]
        );
        assert!(
            store.skip_reasons().unwrap().is_empty(),
            "awaiting OCR is not one of the pre-read skip reasons"
        );
        let row = store
            .file_state(&dir.join("invoice.scan"))
            .unwrap()
            .expect("the file is recorded");
        assert!(!row.extracted, "the row is still pending work");
        assert_eq!(
            row.sha1, None,
            "nothing was extracted, so nothing is claimed"
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
            None,
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
            None,
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

    /// A text embedder with no model behind it: the sweep's bookkeeping is what
    /// these tests exercise, and it must not need 22 MB of ONNX to be tested.
    struct FakeText {
        version: &'static str,
        fail_on: Option<&'static str>,
    }

    impl TextEmbedder for FakeText {
        fn version(&self) -> &str {
            self.version
        }

        fn embed_text(&self, text: &str) -> Result<crate::vectors::Embedding> {
            if self.fail_on.is_some_and(|needle| text.contains(needle)) {
                anyhow::bail!("this tile will not embed");
            }
            let mut values = vec![0f32; crate::store::TEXT_EMBEDDING_DIM];
            values[0] = 1.0;
            crate::vectors::Embedding::new(values, crate::store::TEXT_EMBEDDING_DIM)
        }
    }

    #[test]
    fn the_sweep_embeds_every_tile_and_records_the_model_pin() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        write(corpus.path(), "a.txt", "alpha beta gamma");
        write(corpus.path(), "b.txt", "delta epsilon");
        let store = Store::open(data.path()).unwrap();
        let text = FakeText {
            version: "minilm-pin",
            fail_on: None,
        };

        let summary = index_folder_with(
            &store,
            &[Box::new(TextPipeline)],
            corpus.path(),
            &SystemProbe,
            Some(&text),
        )
        .unwrap();

        assert_eq!(summary.indexed, 2);
        assert_eq!(summary.text_embedded, 2, "one tile per file here");
        assert_eq!(summary.text_unembedded, 0);
        assert_eq!(store.text_embedding_count().unwrap(), 2);
        assert!(store.tiles_without_text_vectors().unwrap().is_empty());
        assert_eq!(
            store.arm_version(Arm::Text).unwrap().as_deref(),
            Some("minilm-pin"),
            "a finished sweep claims the arm for the model that built it"
        );
        assert_eq!(
            store.arm_version(Arm::Image).unwrap(),
            None,
            "the text sweep must not claim the image arm"
        );
    }

    #[test]
    fn a_tile_that_cannot_be_embedded_stays_outstanding_and_the_arm_is_not_claimed() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        write(corpus.path(), "a.txt", "alpha beta");
        write(corpus.path(), "b.txt", "delta epsilon");
        let store = Store::open(data.path()).unwrap();
        let failing = FakeText {
            version: "minilm-pin",
            fail_on: Some("delta"),
        };

        let summary = index_folder_with(
            &store,
            &[Box::new(TextPipeline)],
            corpus.path(),
            &SystemProbe,
            Some(&failing),
        )
        .unwrap();

        assert_eq!(summary.text_embedded, 1);
        assert_eq!(summary.text_unembedded, 1);
        assert_eq!(
            store.arm_version(Arm::Text).unwrap(),
            None,
            "a partial run is outstanding work, not a completed arm"
        );
        assert_eq!(
            store.tiles_without_text_vectors().unwrap().len(),
            1,
            "the failed tile stays in the work list"
        );

        // The next run has nothing to extract and still repairs the arm: the
        // sweep reads the store, not the files.
        let fixed = FakeText {
            version: "minilm-pin",
            fail_on: None,
        };
        let again = index_folder_with(
            &store,
            &[Box::new(TextPipeline)],
            corpus.path(),
            &SystemProbe,
            Some(&fixed),
        )
        .unwrap();
        assert_eq!((again.indexed, again.unchanged), (0, 2));
        assert_eq!(again.text_embedded, 1);
        assert_eq!(store.text_embedding_count().unwrap(), 2);
        assert_eq!(
            store.arm_version(Arm::Text).unwrap().as_deref(),
            Some("minilm-pin")
        );
    }

    #[test]
    fn a_moved_pin_is_refused_rather_than_mixed() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        write(corpus.path(), "a.txt", "alpha beta");
        let store = Store::open(data.path()).unwrap();
        store.set_arm_version(Arm::Text, "old-pin").unwrap();
        let text = FakeText {
            version: "new-pin",
            fail_on: None,
        };

        let err = index_folder_with(
            &store,
            &[Box::new(TextPipeline)],
            corpus.path(),
            &SystemProbe,
            Some(&text),
        )
        .expect_err("an index holding another model's vectors must be refused");
        assert!(
            format!("{err:#}").contains("mixing two vector spaces"),
            "{err:#}"
        );
        assert_eq!(
            store.arm_version(Arm::Text).unwrap().as_deref(),
            Some("old-pin"),
            "the refusal must not have touched the pin"
        );
    }
}
