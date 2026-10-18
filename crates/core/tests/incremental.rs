//! Ticket 02 acceptance: incremental, honest indexing over a real folder.
//!
//! Re-indexing verifies instead of re-extracting, additions and edits take
//! effect, deletions and renames propagate without doubling anything, files
//! that must not be read are never opened, the diagnostics say why, and an
//! interrupted run leaves no half-written rows behind.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use tessera_core::index::skip::MAX_FILE_BYTES;
use tessera_core::index::{index_folder, text::TextPipeline, Extraction, Kind, Pipeline};
use tessera_core::search::search;
use tessera_core::store::Store;

/// Records every file a pipeline was asked to open, so a test can prove which
/// files had their contents read and which were never touched. `crash_after`
/// makes the Nth file panic mid-run — a stand-in for a crash that stops the
/// engine between files.
#[derive(Default)]
struct Recorder {
    opened: Arc<Mutex<Vec<PathBuf>>>,
    crash_after: Mutex<Option<usize>>,
}

impl Recorder {
    fn opened(&self) -> Vec<PathBuf> {
        self.opened.lock().unwrap().clone()
    }

    fn stop_crashing(&self) {
        *self.crash_after.lock().unwrap() = None;
    }
}

struct Recorded(Arc<Recorder>);

impl Pipeline for Recorded {
    fn accepts(&self, path: &Path) -> Option<Kind> {
        TextPipeline.accepts(path)
    }

    fn process(&self, path: &Path) -> Result<Extraction> {
        let opened = {
            let mut opened = self.0.opened.lock().unwrap();
            opened.push(path.to_path_buf());
            opened.len()
        };
        if self
            .0
            .crash_after
            .lock()
            .unwrap()
            .is_some_and(|at| opened >= at)
        {
            panic!("simulated crash while indexing {}", path.display());
        }
        TextPipeline.process(path)
    }
}

fn write(dir: &Path, name: &str, contents: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(name), contents).unwrap();
}

/// Index `dir` with a pipeline that reports what it opened.
fn run(store: &Store, recorder: &Arc<Recorder>, dir: &Path) -> tessera_core::index::IndexSummary {
    index_folder(store, &[Box::new(Recorded(recorder.clone()))], dir).unwrap()
}

fn hits(store: &Store, term: &str) -> Vec<String> {
    search(store, &[term])
        .unwrap()
        .into_iter()
        .map(|hit| hit.path)
        .collect()
}

fn only_hit(store: &Store, term: &str) -> String {
    let found = hits(store, term);
    assert_eq!(found.len(), 1, "expected exactly one hit for {term:?}");
    found.into_iter().next().unwrap()
}

fn store_at(dir: &Path) -> Store {
    Store::open(dir).unwrap()
}

#[test]
fn a_second_index_run_verifies_without_reading_anything() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    write(dir, "a.txt", "the invoice from the airline");
    write(dir, "b.md", "zebra crossing ahead");

    let store = store_at(data.path());
    let recorder = Arc::new(Recorder::default());

    let first = run(&store, &recorder, dir);
    assert_eq!(first.indexed, 2);
    assert_eq!(first.unchanged, 0);
    assert_eq!(recorder.opened().len(), 2);

    // Nothing changed. The run must stat, not read: re-extracting a 20k-file
    // corpus on every launch is the failure mode this ticket exists to stop.
    let second = run(&store, &recorder, dir);
    assert_eq!(
        second,
        tessera_core::index::IndexSummary {
            unchanged: 2,
            ..Default::default()
        }
    );
    assert_eq!(recorder.opened().len(), 2, "a second run opened a file");

    // Still searchable, still one row each.
    assert_eq!(hits(&store, "airline").len(), 1);
    let stats = store.stats().unwrap();
    assert_eq!(stats, vec![("indexed".to_string(), 2)]);
}

#[test]
fn a_touched_but_unchanged_file_is_not_re_extracted() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    let path = dir.join("a.txt");
    write(dir, "a.txt", "the invoice from the airline");

    let store = store_at(data.path());
    let recorder = Arc::new(Recorder::default());
    run(&store, &recorder, dir);

    // Same bytes, new mtime — what an editor save or a copy does.
    let future = std::time::SystemTime::now() + std::time::Duration::from_secs(120);
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, &bytes).unwrap();
    let file = fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(future).unwrap();
    drop(file);

    let summary = run(&store, &recorder, dir);
    assert_eq!(
        summary.unchanged, 1,
        "the hash proves the bytes did not change"
    );
    assert_eq!(summary.indexed, 0);
    assert_eq!(
        recorder.opened().len(),
        1,
        "re-extracted a file whose content is identical"
    );
}

#[test]
fn adding_editing_and_deleting_all_take_effect() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    write(dir, "a.txt", "the invoice from the airline arrived");

    let store = store_at(data.path());
    let recorder = Arc::new(Recorder::default());

    assert_eq!(run(&store, &recorder, dir).indexed, 1);
    assert_eq!(
        only_hit(&store, "airline"),
        dir.join("a.txt").to_string_lossy()
    );

    // Added: indexed on the next run.
    write(dir, "b.txt", "zebra crossing");
    let added = run(&store, &recorder, dir);
    assert_eq!((added.indexed, added.unchanged), (1, 1));
    assert_eq!(
        only_hit(&store, "zebra"),
        dir.join("b.txt").to_string_lossy()
    );

    // Edited: re-extracted, and the old words are gone.
    write(dir, "a.txt", "the invoice from the railway now");
    let edited = run(&store, &recorder, dir);
    assert_eq!((edited.indexed, edited.unchanged), (1, 1));
    assert!(
        hits(&store, "airline").is_empty(),
        "the replaced text is still findable"
    );
    assert_eq!(
        only_hit(&store, "railway"),
        dir.join("a.txt").to_string_lossy()
    );

    // Deleted: gone from results, and gone from the index.
    fs::remove_file(dir.join("b.txt")).unwrap();
    let deleted = run(&store, &recorder, dir);
    assert_eq!(deleted.removed, 1);
    assert!(hits(&store, "zebra").is_empty());
    assert_eq!(store.stats().unwrap(), vec![("indexed".to_string(), 1)]);
}

#[test]
fn a_rename_moves_the_row_and_keeps_it_findable_by_its_new_name() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    write(dir, "scan-0042.txt", "the invoice from the airline arrived");

    let store = store_at(data.path());
    let recorder = Arc::new(Recorder::default());
    assert_eq!(run(&store, &recorder, dir).indexed, 1);

    fs::rename(dir.join("scan-0042.txt"), dir.join("airline-june.txt")).unwrap();
    let moved = run(&store, &recorder, dir);
    assert_eq!(
        moved.moved, 1,
        "a move is recognised by content, not re-extracted"
    );
    assert_eq!((moved.indexed, moved.unchanged, moved.removed), (0, 0, 0));
    assert_eq!(
        recorder.opened().len(),
        1,
        "the moved file was opened again"
    );
    assert_eq!(
        store.stats().unwrap(),
        vec![("indexed".to_string(), 1)],
        "the row doubled"
    );

    // Same content, new name — findable by both, and by the new name only.
    let new_path = dir.join("airline-june.txt").to_string_lossy().to_string();
    assert_eq!(only_hit(&store, "airline-june"), new_path);
    assert_eq!(only_hit(&store, "arrived"), new_path);
    assert!(
        hits(&store, "scan-0042").is_empty(),
        "the old name is still indexed"
    );
}

#[test]
fn a_move_into_another_folder_of_the_same_corpus_is_still_a_move() {
    // Every other move test renames within one directory. "Moving" a file in
    // practice means moving it somewhere else — `Documents/2024/` — and that is
    // a different path shape through the same logic, so it gets its own test
    // rather than being assumed to follow from the rename case.
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    let archive = dir.join("archive");
    fs::create_dir_all(&archive).unwrap();
    write(dir, "scan-0042.txt", "the invoice from the airline arrived");

    let store = store_at(data.path());
    let recorder = Arc::new(Recorder::default());
    assert_eq!(run(&store, &recorder, dir).indexed, 1);

    fs::rename(dir.join("scan-0042.txt"), archive.join("airline-june.txt")).unwrap();
    let moved = run(&store, &recorder, dir);
    assert_eq!(
        (moved.moved, moved.indexed, moved.unchanged, moved.removed),
        (1, 0, 0, 0),
        "a file moved between folders of one corpus is recognised by content"
    );
    assert_eq!(
        recorder.opened().len(),
        1,
        "the moved file was extracted again instead of moved"
    );
    assert_eq!(store.stats().unwrap(), vec![("indexed".to_string(), 1)]);

    // Findable at its new path, under its new name, and by the text it holds —
    // all three, because a move updates the path and re-titles the first tile.
    let moved_to = archive
        .join("airline-june.txt")
        .to_string_lossy()
        .to_string();
    assert_eq!(only_hit(&store, "airline-june"), moved_to);
    assert_eq!(only_hit(&store, "arrived"), moved_to);
}

#[test]
fn two_live_copies_of_the_same_bytes_stay_two_files() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    write(dir, "a.txt", "identical content here");
    write(dir, "b.txt", "identical content here");

    let store = store_at(data.path());
    let recorder = Arc::new(Recorder::default());
    let first = run(&store, &recorder, dir);
    assert_eq!(
        (first.indexed, first.moved),
        (2, 0),
        "a live duplicate is not a move"
    );

    // Renaming one of them moves that one; the other stays where it is.
    fs::rename(dir.join("b.txt"), dir.join("c.txt")).unwrap();
    let second = run(&store, &recorder, dir);
    assert_eq!((second.moved, second.indexed, second.removed), (1, 0, 0));
    assert_eq!(store.stats().unwrap(), vec![("indexed".to_string(), 2)]);
    assert_eq!(hits(&store, "identical").len(), 2);
}

#[test]
fn a_file_over_the_cap_is_skipped_with_its_reason() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    write(dir, "small.txt", "the invoice from the airline");
    // A sparse file: the cap is exercised without spending 50 MB of disk.
    let huge = fs::File::create(dir.join("huge.txt")).unwrap();
    huge.set_len(MAX_FILE_BYTES + 1).unwrap();
    drop(huge);

    let store = store_at(data.path());
    let recorder = Arc::new(Recorder::default());
    let summary = run(&store, &recorder, dir);
    assert_eq!((summary.indexed, summary.skipped), (1, 1));
    assert_eq!(
        recorder.opened().len(),
        1,
        "an oversize file was read: it would have been chunked into memory"
    );
    assert_eq!(
        store.skip_reasons().unwrap(),
        vec![("too_large".to_string(), 1)]
    );
    assert_eq!(
        store.stats().unwrap(),
        vec![("indexed".to_string(), 1), ("skipped".to_string(), 1)]
    );
}

#[test]
fn indexing_a_folder_that_is_not_there_is_a_loud_error() {
    let data = tempfile::tempdir().unwrap();
    let store = store_at(data.path());
    let missing = data.path().join("no-such-folder");
    let err = index_folder(&store, &[Box::new(TextPipeline)], &missing);
    assert!(
        err.is_err(),
        "a vanished folder must not look like an empty one"
    );
    assert!(store.stats().unwrap().is_empty());
}

#[test]
fn a_deleted_file_is_only_forgotten_when_its_own_folder_is_scanned() {
    let data = tempfile::tempdir().unwrap();
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    write(first_root.path(), "a.txt", "the invoice from the airline");
    write(second_root.path(), "b.txt", "zebra crossing");

    let store = store_at(data.path());
    assert_eq!(
        index_folder(&store, &[Box::new(TextPipeline)], first_root.path())
            .unwrap()
            .indexed,
        1
    );
    assert_eq!(
        index_folder(&store, &[Box::new(TextPipeline)], second_root.path())
            .unwrap()
            .indexed,
        1
    );

    // Scanning one folder must not forget another: the store is shared, the
    // corpora are not.
    fs::remove_file(first_root.path().join("a.txt")).unwrap();
    let other = index_folder(&store, &[Box::new(TextPipeline)], second_root.path()).unwrap();
    assert_eq!(other.removed, 0);
    assert_eq!(
        hits(&store, "airline").len(),
        1,
        "another folder's row was deleted"
    );

    // Its own folder is where the deletion propagates.
    let own = index_folder(&store, &[Box::new(TextPipeline)], first_root.path()).unwrap();
    assert_eq!(own.removed, 1);
    assert!(hits(&store, "airline").is_empty());
    assert_eq!(hits(&store, "zebra").len(), 1);
}

#[test]
fn a_run_interrupted_mid_index_leaves_only_committed_work() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    // Distinctive text per file, so "what landed" is a claim, not a guess.
    write(dir, "first.txt", "alpha content");
    write(dir, "second.txt", "bravo content");
    write(dir, "third.txt", "charlie content");
    let text_of = |name: &str| match name {
        "first.txt" => "alpha",
        "second.txt" => "bravo",
        _ => "charlie",
    };

    let store = store_at(data.path());
    let recorder = Arc::new(Recorder::default());
    // Stop on the second file the run opens: whatever walk order we get, the
    // first file is fully committed and the second one is interrupted, so the
    // assertions below hold in every order rather than in the happy one.
    *recorder.crash_after.lock().unwrap() = Some(2);

    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        index_folder(&store, &[Box::new(Recorded(recorder.clone()))], dir)
    }));
    assert!(crashed.is_err(), "the simulated crash did not happen");
    let opened = recorder.opened();
    assert_eq!(
        opened.len(),
        2,
        "the run must have stopped on the second file"
    );
    let (committed, interrupted) = (&opened[0], &opened[1]);
    let committed_text = text_of(committed.file_name().unwrap().to_str().unwrap());
    let interrupted_text = text_of(interrupted.file_name().unwrap().to_str().unwrap());

    // The committed file is there, whole, and searchable by its own text.
    assert_eq!(
        only_hit(&store, committed_text),
        committed.to_string_lossy(),
        "the file indexed before the crash is not searchable"
    );
    assert!(
        hits(&store, interrupted_text).is_empty(),
        "the interrupted file left rows behind"
    );

    // And the engine recovers: the next run finishes the job.
    recorder.stop_crashing();
    let recovered = run(&store, &recorder, dir);
    assert_eq!(recovered.removed, 0);
    for name in ["first.txt", "second.txt", "third.txt"] {
        assert_eq!(
            only_hit(&store, text_of(name)),
            dir.join(name).to_string_lossy(),
            "{name} is not searchable after recovery"
        );
    }
    assert_eq!(store.stats().unwrap(), vec![("indexed".to_string(), 3)]);
}

#[test]
fn an_edited_file_of_the_same_length_is_re_extracted() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    let path = dir.join("config.txt");
    // Same length, new content: the size is useless here, and only the
    // timestamp can tell the two states apart. Second-resolution mtimes cannot
    // (two writes in the same tick look untouched), which is why the engine
    // stores nanoseconds.
    write(dir, "config.txt", "port=8080");

    let store = store_at(data.path());
    let recorder = Arc::new(Recorder::default());
    assert_eq!(run(&store, &recorder, dir).indexed, 1);

    write(dir, "config.txt", "port=9090");
    let summary = run(&store, &recorder, dir);
    assert_eq!((summary.indexed, summary.unchanged), (1, 0));
    assert_eq!(only_hit(&store, "9090"), path.to_string_lossy());
    assert!(
        hits(&store, "8080").is_empty(),
        "the replaced text is still findable"
    );
}

#[test]
fn a_folder_whose_name_starts_like_another_is_not_swept_with_it() {
    let data = tempfile::tempdir().unwrap();
    let top = tempfile::tempdir().unwrap();
    let corpus = top.path().join("corpus");
    let lookalike = top.path().join("corpus-old");
    write(&corpus, "mine.txt", "the invoice from the airline");
    write(&lookalike, "theirs.txt", "zebra crossing");

    let store = store_at(data.path());
    index_folder(&store, &[Box::new(TextPipeline)], &corpus).unwrap();
    index_folder(&store, &[Box::new(TextPipeline)], &lookalike).unwrap();

    // Delete a file from the sibling, then scan only the shorter-named folder.
    // A prefix match that forgets the separator would take the sibling's row
    // with it.
    fs::remove_file(lookalike.join("theirs.txt")).unwrap();
    let summary = index_folder(&store, &[Box::new(TextPipeline)], &corpus).unwrap();
    assert_eq!(
        summary.removed, 0,
        "swept a sibling folder whose name shares a prefix"
    );
    assert_eq!(
        hits(&store, "zebra").len(),
        1,
        "the sibling folder's row was deleted by another folder's scan"
    );
}
