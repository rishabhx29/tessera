//! Ticket 01 acceptance: the engine's first complete path, exercised
//! end-to-end over a real folder — index text files, search typo-tolerantly,
//! match multi-word queries across tiles, find files by name, re-index without
//! duplicates, and report honest stats.

use std::fs;
use std::path::Path;

use tessera_core::cli::index_text_folder;
use tessera_core::fusion::parse_query;
use tessera_core::models::Models;
use tessera_core::search::search;
use tessera_core::store::Store;

/// The engine's search terms, the way the CLI derives them from arguments.
fn terms_from(args: &[&str]) -> Vec<String> {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    parse_query(&args).expect("the test queries parse").terms
}

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

/// An empty models directory, which is what a machine that has never run
/// `tessera models install` looks like. These tests are about the text pipeline,
/// and an absent OCR engine must not change their answers.
fn no_models(data: &Path) -> Models {
    Models::open(&data.join("models")).expect("an empty models dir is always openable")
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
    // A file findable only by its name.
    write(dir, "notes/zwiebel-recipe.txt", "boil water, add pasta");
    // A file that must not match anything.
    write(dir, "other.log", "temperature 21.5 humidity 40");
    // A file that fails extraction (invalid UTF-8) and is recorded, not fatal.
    fs::write(dir.join("bad.txt"), [0xFF, 0xFE, 0x00, 0xD8]).unwrap();

    let store = Store::open(data.path()).unwrap();
    let summary = index_text_folder(&store, &no_models(data.path()), dir).unwrap();
    assert_eq!(summary.indexed, 4, "four readable text files");
    assert_eq!(summary.failed, 1, "the invalid-UTF-8 file fails honestly");
    assert_eq!(summary.walk_errors, 0);

    // A word from the first file finds it, with a snippet.
    let expected_a = dir
        .join("notes")
        .join("a.txt")
        .to_string_lossy()
        .to_string();
    let hits = search(&store, &["airline"]).unwrap();
    assert_eq!(paths_of(&hits), vec![expected_a.clone()]);
    assert!(hits[0].snippet.contains("invoice"));

    // Typo tolerance: a real lookalike typo (0 for o) still finds the file.
    let hits = search(&store, &["inv0ice"]).unwrap();
    assert_eq!(
        paths_of(&hits),
        vec![expected_a.clone()],
        "'inv0ice' must find the invoice file"
    );

    // Partial word: substring matching via the trigram tokenizer.
    let hits = search(&store, &["invoic"]).unwrap();
    assert_eq!(paths_of(&hits), vec![expected_a]);

    // Multi-word query where the words live in DIFFERENT tiles of one file.
    let hits = search(&store, &["zebra", "quixote"]).unwrap();
    assert_eq!(
        paths_of(&hits).len(),
        1,
        "terms matched across tiles of long.md"
    );

    // The CLI splits a quoted query into terms; those terms still AND together.
    let terms = terms_from(&["zebra quixote"]);
    let hits = search(
        &store,
        &terms.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(paths_of(&hits).len(), 1);

    // Multi-word AND semantics: one matching word alone is not enough.
    let hits = search(&store, &["zebra", "nonsense"]).unwrap();
    assert!(hits.is_empty());

    // Punctuation in a term is literal text, never FTS5 syntax.
    let hits = search(&store, &["QF-3391"]).unwrap();
    assert_eq!(paths_of(&hits).len(), 1);

    // A file is findable by its name even when its content says nothing.
    let hits = search(&store, &["zwiebel"]).unwrap();
    assert_eq!(paths_of(&hits).len(), 1);
    assert!(paths_of(&hits)[0].contains("zwiebel-recipe.txt"));

    // Re-indexing the same folder verifies without re-extracting anything.
    let again = index_text_folder(&store, &no_models(data.path()), dir).unwrap();
    assert_eq!(again.indexed, 0, "nothing changed, so nothing is extracted");
    assert_eq!(again.unchanged, 4);
    assert_eq!(
        again.failed, 1,
        "a failed file is retried, not remembered as done"
    );
    let hits = search(&store, &["airline"]).unwrap();
    assert_eq!(paths_of(&hits).len(), 1, "no duplicates after re-index");

    // Stats tell the truth.
    let stats = store.stats().unwrap();
    assert!(stats.contains(&("failed".to_string(), 1)));
    assert!(stats.contains(&("indexed".to_string(), 4)));

    // A failed file never becomes searchable by its garbage bytes.
    let hits = search(&store, &["bad"]).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn empty_corpus_and_empty_query_are_safe() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let store = Store::open(data.path()).unwrap();
    let summary = index_text_folder(&store, &no_models(data.path()), corpus.path()).unwrap();
    assert_eq!(summary, tessera_core::index::IndexSummary::default());
    assert!(search(&store, &[]).unwrap().is_empty());
    assert!(search(&store, &["anything"]).unwrap().is_empty());
    let stats = store.stats().unwrap();
    assert!(stats.is_empty());
}
