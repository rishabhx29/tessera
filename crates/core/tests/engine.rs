//! Ticket 01 acceptance: the engine's first complete path, exercised
//! end-to-end over a real folder — index text files, search typo-tolerantly,
//! match multi-word queries across tiles, re-index without duplicates, and
//! report honest stats.

use std::fs;
use std::path::Path;

use tessera_core::cli::index_text_folder;
use tessera_core::search::search;
use tessera_core::store::Store;

fn write(dir: &Path, name: &str, contents: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn paths_of(hits: &[tessera_core::search::Hit]) -> Vec<String> {
    hits.iter().map(|h| h.path.clone()).collect()
}

#[test]
fn index_search_reindex_stats_end_to_end() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();

    // A file whose text is findable, plus a typo variant of a word.
    write(
        dir,
        "notes/a.txt",
        "The invoice from the airline arrived. Booking reference QF-3391.",
    );
    // A long file whose words land in different tiles.
    let filler: String = (0..400).map(|i| format!("filler{i} ")).collect();
    let long = format!("{filler}zebra crossing ahead {filler}don quixote rode here");
    write(dir, "long.md", &long);
    // A file that must not match anything.
    write(dir, "other.log", "temperature 21.5 humidity 40");
    // A file that fails extraction (invalid UTF-8) and is recorded, not fatal.
    fs::write(dir.join("bad.txt"), [0xFF, 0xFE, 0x00, 0xD8]).unwrap();

    let store = Store::open(data.path()).unwrap();
    let (indexed, failed) = index_text_folder(&store, dir).unwrap();
    assert_eq!(indexed, 3, "three readable text files");
    assert_eq!(failed, 1, "the invalid-UTF-8 file fails honestly");

    // A word from the first file finds it, with a snippet.
    let expected_a = dir
        .join("notes")
        .join("a.txt")
        .to_string_lossy()
        .to_string();
    let hits = search(&store, &["airline".to_string()]).unwrap();
    assert_eq!(paths_of(&hits), vec![expected_a.clone()]);
    assert!(hits[0].snippet.contains("invoice"));

    // Typo tolerance via the trigram tokenizer: a partial word still matches.
    let hits = search(&store, &["invoic".to_string()]).unwrap();
    assert_eq!(
        paths_of(&hits).len(),
        1,
        "partial 'invoic' finds the invoice file"
    );

    // Multi-word query where the words live in DIFFERENT tiles of one file.
    let hits = search(&store, &["zebra".to_string(), "quixote".to_string()]).unwrap();
    assert_eq!(
        paths_of(&hits).len(),
        1,
        "terms matched across tiles of long.md"
    );

    // Multi-word AND semantics: one matching word alone is not enough.
    let hits = search(&store, &["zebra".to_string(), "nonsense".to_string()]).unwrap();
    assert!(hits.is_empty());

    // Punctuation in a term is literal text, never FTS5 syntax.
    let hits = search(&store, &["QF-3391".to_string()]).unwrap();
    assert_eq!(paths_of(&hits).len(), 1);

    // Re-indexing the same folder changes nothing.
    let (indexed_again, failed_again) = index_text_folder(&store, dir).unwrap();
    assert_eq!((indexed_again, failed_again), (3, 1));
    let hits = search(&store, &["airline".to_string()]).unwrap();
    assert_eq!(paths_of(&hits).len(), 1, "no duplicates after re-index");

    // Stats tell the truth.
    let stats = store.stats().unwrap();
    assert!(stats.contains(&("failed".to_string(), 1)));
    assert!(stats.contains(&("indexed".to_string(), 3)));

    // A failed file never becomes searchable by its garbage bytes.
    let hits = search(&store, &["bad".to_string()]).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn empty_corpus_and_empty_query_are_safe() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let store = Store::open(data.path()).unwrap();
    let (indexed, failed) = index_text_folder(&store, corpus.path()).unwrap();
    assert_eq!((indexed, failed), (0, 0));
    assert!(search(&store, &[]).unwrap().is_empty());
    assert!(search(&store, &["anything".to_string()])
        .unwrap()
        .is_empty());
    let stats = store.stats().unwrap();
    assert!(stats.is_empty());
}
