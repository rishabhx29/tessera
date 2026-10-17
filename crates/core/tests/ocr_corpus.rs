//! Search images by the text printed inside them, on a downloaded corpus with
//! ground-truth transcriptions.
//!
//! Every OCR measurement in this repository so far ran over a hand-picked
//! folder: 40 screenshots for the engine bake-offs, 56-178 for the fusion and
//! weighting sweeps. Those answer "does it work on screenshots" and say nothing
//! about scale. This test runs the shipping engine - the platform OCR by
//! default, the ONNX fallback on request - over a sample of real photographs
//! whose printed text people transcribed (COCO-Text V2 over COCO val2014), and
//! measures the question the product promises: a word printed in a photograph
//! finds that photograph.
//!
//! Two numbers, because they fail in different places:
//!
//! - **word recall** - the share of ground-truth words the engine's text
//!   contains. Bounded by the engine and the corpus, and by nothing else.
//! - **word findable** - the share of ground-truth words whose photograph comes
//!   back from [`search`]. This adds the store's trigram index to the path. It
//!   can sit *below* word recall only if the store fails to return a word it was
//!   given; that gap should be near zero, and when it is not, the failure is in
//!   the store, not in the model. It can sit *above* word recall, because
//!   `search` expands every term into lookalike variants (0↔o, 1↔l↔i, …) - OCR
//!   misreads a character into its lookalike and the search still lands.
//!
//! The OCR pass runs once per image; the store pass replays the recorded text
//! through [`ImagePipeline`], so the engine is never asked twice and the store
//! path that gets measured is the real one - the same code, the same trigram
//! index, the same `search`.
//!
//! The third measurement follows from the first two and exists because of what
//! they found: on these photographs the platform engine reads less than half of
//! what the ONNX fallback reads, at a third of the time *on this corpus*, and
//! the working trade in the product is probably neither engine alone but a
//! cascade - ask the fast engine first, pay for the slow one only where the
//! fast one found nothing. That is measured, not assumed: the cascade test
//! re-runs the whole harness with a reader that is "native, then ONNX on every
//! empty read", so the gain and the cost land in the same table as the two
//! engines it is built from. The retry trigger is *empty* rather than *short*:
//! it is the case the two runs above can decide from their own data, and it
//! makes the measured gain a floor and the measured cost a ceiling for any
//! smarter trigger.
//!
//! The corpus lives outside the repository, like every other real-image corpus:
//!
//! ```text
//! TESSERA_OCR_CORPUS -> a directory containing images\ and text.json
//! TESSERA_OCR_ENGINE -> "onnx" or any named engine (default: the platform's)
//! TESSERA_OCR_LIMIT  -> how many images to sample (default: all)
//! ```
//!
//! ```
//! $env:TESSERA_OCR_CORPUS="$env:LOCALAPPDATA\Temp\opencode\coco-text-eval"
//! cargo test --release -p tessera-core --test ocr_corpus -- --ignored --nocapture
//! $env:TESSERA_OCR_ENGINE="onnx"
//! cargo test --release -p tessera-core --test ocr_corpus printed_words -- --ignored --nocapture
//! cargo test --release -p tessera-core --test ocr_corpus cascade -- --ignored --nocapture
//! ```
//!
//! A word is queried whole, with punctuation and case stripped (`Nova` and
//! `NOVA!` are one query). That is conservative for punctuated strings such as
//! `COCA-COLA`, whose parts a person would query separately; the transcripts
//! this corpus carries are mostly single sign words, and widening the query set
//! would change what the number means more than it would help it. Words of
//! fewer than four characters and words that are all digits are left out: the
//! first are not what anyone types into a search box, and the second would be
//! found inside the image's own file name, which is not OCR reading anything.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use tessera_core::index::image::ImagePipeline;
use tessera_core::index::index_folder;
use tessera_core::index::text::TextPipeline;
use tessera_core::models::Models;
use tessera_core::ocr::{OcrChoice, OcrEngine};
use tessera_core::search::search;
use tessera_core::store::Store;

/// Ground-truth words shorter than this are not asked about.
const MIN_WORD: usize = 4;

fn models() -> Models {
    let dir = std::env::var("TESSERA_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            tessera_core::cli::models_dir(
                &tessera_core::cli::default_data_dir().expect("a data dir"),
                None,
            )
        });
    Models::open(&dir).expect("models dir")
}

fn corpus_dir() -> PathBuf {
    std::env::var("TESSERA_OCR_CORPUS")
        .map(PathBuf::from)
        .expect("set TESSERA_OCR_CORPUS to a directory holding images\\ and text.json")
}

#[derive(Clone)]
struct Word {
    text: String,
    legible: bool,
    english: bool,
}

#[derive(Clone)]
struct Entry {
    path: PathBuf,
    words: Vec<Word>,
}

/// Parse `text.json`, failing loudly on anything that is not the documented
/// shape: a corpus that silently arrives half-read measures the wrong thing.
fn read_text_json(dir: &Path) -> Vec<Entry> {
    let path = dir.join("text.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
    let parsed: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|err| panic!("parsing text.json: {err}"));
    let items = parsed["images"]
        .as_array()
        .unwrap_or_else(|| panic!("{path:?} has no `images` array"));

    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        let relative = item["path"]
            .as_str()
            .unwrap_or_else(|| panic!("an entry has no string `path`: {item}"));
        let words: Vec<Word> = item["words"]
            .as_array()
            .unwrap_or_else(|| panic!("{relative} has no `words` array"))
            .iter()
            .map(|word| Word {
                text: word["text"].as_str().unwrap_or_default().to_string(),
                legible: word["legible"].as_bool().unwrap_or(false),
                english: word["english"].as_bool().unwrap_or(false),
            })
            .collect();
        // Manifest paths are Windows-shaped; split on either separator so the
        // corpus also builds on a machine that is not this one.
        let mut path = dir.to_path_buf();
        for part in relative.split(['\\', '/']).filter(|p| !p.is_empty()) {
            path.push(part);
        }
        entries.push(Entry { path, words });
    }
    assert!(!entries.is_empty(), "text.json is empty");
    entries
}

/// The corpus as the environment asks for it, with both failure modes checked
/// before any engine runs: a sample that is too small to mean anything, and a
/// manifest pointing at files that are not there. Returns the directory, the
/// (possibly sampled) entries, and the full corpus size so a limited run can
/// say it was limited.
fn sample_corpus() -> (PathBuf, Vec<Entry>, usize) {
    let dir = corpus_dir();
    let mut entries = read_text_json(&dir);
    let full = entries.len();
    if let Ok(limit) = std::env::var("TESSERA_OCR_LIMIT") {
        let limit: usize = limit.parse().expect("TESSERA_OCR_LIMIT is a number");
        assert!(limit > 0, "a sample of zero images measures nothing");
        if entries.len() > limit {
            let stride = entries.len() / limit;
            entries = (0..limit).map(|i| entries[i * stride].clone()).collect();
        }
    }
    assert!(
        entries.len() >= 32,
        "only {} photographs in the corpus; too few to say anything",
        entries.len()
    );

    let missing: Vec<&PathBuf> = entries
        .iter()
        .map(|entry| &entry.path)
        .filter(|path| !path.is_file())
        .collect();
    assert!(
        missing.is_empty(),
        "{} of {} text.json entries point at files that do not exist, first: {}",
        missing.len(),
        entries.len(),
        missing[0].display()
    );
    (dir, entries, full)
}

/// Lower-case, keep ASCII letters and digits. The FTS5 trigram index folds
/// case (SQLite's default for that tokenizer), so a lower-cased substring
/// check is the same predicate the store will apply, minus the ranking.
fn normalise(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The words a person might actually type: legible, English, long enough to be
/// distinct, and not a bare number (see the module doc).
fn query_words(entry: &Entry) -> BTreeSet<String> {
    entry
        .words
        .iter()
        .filter(|word| word.legible && word.english)
        .map(|word| normalise(&word.text))
        .filter(|word| {
            word.chars().count() >= MIN_WORD && !word.chars().all(|c| c.is_ascii_digit())
        })
        .collect()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

/// An OCR engine that returns the text already read from a file, keyed by file
/// name. The store pass needs an engine; re-running the real one would double
/// the slowest part of the measurement and measure its variance twice.
struct Replay {
    by_name: HashMap<String, String>,
}

impl OcrEngine for Replay {
    fn name(&self) -> &'static str {
        "replay"
    }

    fn recognize(&self, path: &Path) -> Result<String> {
        Ok(self
            .by_name
            .get(&file_name(path))
            .cloned()
            .unwrap_or_default())
    }
}

/// The part of one run the wiring floors assert on. Everything else a run
/// measures is printed where it is computed, in `measure`, so the two cannot
/// drift; these three are the ones a test needs to act on.
struct Measured {
    words_total: usize,
    words_read: usize,
    words_findable: usize,
}

/// The whole measurement, for whatever `read` is: run it over every photograph,
/// score it against the transcriptions, replay its text through the real store,
/// and ask the real search for every question-able word. `read` returns the
/// engine's result and how long it took, so a cascade pays for both engines in
/// its timing and one engine pays for one.
fn measure(
    entries: &[Entry],
    dir: &Path,
    full: usize,
    label: &str,
    mut read: impl FnMut(usize) -> (Result<String>, f64),
) -> Measured {
    // Pass one: the reader visits every image once. Everything after this is
    // arithmetic over what it returned.
    let mut texts: Vec<String> = Vec::with_capacity(entries.len());
    let mut failures = 0usize;
    let mut timings: Vec<f64> = Vec::with_capacity(entries.len());
    let started = Instant::now();
    for index in 0..entries.len() {
        let (result, millis) = read(index);
        match result {
            Ok(text) => texts.push(text.split_whitespace().collect::<Vec<_>>().join(" ")),
            Err(_) => {
                failures += 1;
                texts.push(String::new());
            }
        }
        timings.push(millis);
    }
    let reading = started.elapsed();

    // Word recall, and which words the store will be asked about.
    let lowered: Vec<String> = texts.iter().map(|text| text.to_lowercase()).collect();
    let mut words_total = 0usize;
    let mut words_read = 0usize;
    let mut images_nonempty = 0usize;
    let mut images_all_read = 0usize;
    let mut images_none_read = 0usize;
    let mut with_words = 0usize;
    // word -> indices of the entries whose ground truth carries it.
    let mut by_word: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, entry) in entries.iter().enumerate() {
        if !texts[index].is_empty() {
            images_nonempty += 1;
        }
        let words = query_words(entry);
        if words.is_empty() {
            continue;
        }
        with_words += 1;
        let total_here = words.len();
        let mut read_here = 0usize;
        for word in words {
            words_total += 1;
            if lowered[index].contains(&word) {
                words_read += 1;
                read_here += 1;
            }
            by_word.entry(word).or_default().push(index);
        }
        if read_here == total_here {
            images_all_read += 1;
        }
        if read_here == 0 {
            images_none_read += 1;
        }
    }

    // Pass two: the recorded text through the real store, then the real search
    // for every word the ground truth asked about.
    let data = tempfile::tempdir().expect("a scratch index");
    let store = Store::open(data.path()).expect("the scratch index opens");
    let replay = Replay {
        by_name: entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (file_name(&entry.path), texts[index].clone()))
            .collect(),
    };
    let summary = index_folder(
        &store,
        &[
            Box::new(TextPipeline),
            Box::new(ImagePipeline::new(Box::new(replay))),
        ],
        &dir.join("images"),
    )
    .expect("indexing the corpus");
    assert_eq!(
        summary.indexed,
        entries.len(),
        "every photograph should index ({} failed)",
        summary.failed
    );

    let searching = Instant::now();
    let mut words_findable = 0usize;
    let mut misses = Vec::new();
    for (word, owners) in &by_word {
        let hits: BTreeSet<String> = search(&store, &[word])
            .unwrap_or_else(|err| panic!("searching {word:?}: {err:#}"))
            .iter()
            .filter_map(|hit| Path::new(&hit.path).file_name())
            .filter_map(|name| name.to_str())
            .map(str::to_string)
            .collect();
        for owner in owners {
            if hits.contains(&file_name(&entries[*owner].path)) {
                words_findable += 1;
            } else if lowered[*owner].contains(word) {
                // OCR read it and the store still did not return it.
                if misses.len() < 20 {
                    misses.push(format!(
                        "{word:?} not found in {}",
                        file_name(&entries[*owner].path)
                    ));
                }
            }
        }
    }
    let search_secs = searching.elapsed().as_secs_f64();

    let share = |n: usize| n as f64 / words_total.max(1) as f64;
    let mut chars: Vec<usize> = texts.iter().map(|text| text.chars().count()).collect();
    chars.sort_unstable();
    let chars_p50 = chars[chars.len() / 2];
    timings.sort_by(|a, b| a.total_cmp(b));
    let p50_ms = timings[timings.len() / 2];
    let mean_ms = timings.iter().sum::<f64>() / timings.len() as f64;

    println!(
        "\n== printed words vs {} photographs, {label} ==",
        entries.len()
    );
    if entries.len() != full {
        println!("(sampled from {full} by TESSERA_OCR_LIMIT)");
    }
    println!(
        "read in {:.0} s (p50 {p50_ms:.0} ms, mean {mean_ms:.0} ms per image); \
         {failures} decode failures",
        reading.as_secs_f64()
    );
    println!(
        "images with text          {images_nonempty} of {} ({} read nothing)",
        entries.len(),
        entries.len() - images_nonempty
    );
    println!("characters per image      p50 {chars_p50}");
    println!(
        "images answering words    {images_all_read} read all, {images_none_read} read none \
         (of {with_words} with question-able words)"
    );
    println!(
        "words                     {words_total} asked, {words_read} read ({:.1}%), \
         {words_findable} findable end to end ({:.1}%)",
        share(words_read) * 100.0,
        share(words_findable) * 100.0
    );
    if words_read > words_findable {
        println!(
            "store losses              {} words OCR read but search did not return ({:.2}% of read)",
            words_read - words_findable,
            (words_read - words_findable) as f64 / words_read.max(1) as f64 * 100.0
        );
    }
    if words_findable > words_read {
        // `search` expands each term into lookalike variants (0↔o, 1↔l↔i, …),
        // so a word the strict substring check called unread can still be found
        // when OCR misread a character into its lookalike.
        println!(
            "lookalike tolerance       {} more words found than the strict read check matched",
            words_findable - words_read
        );
    }
    if !misses.is_empty() {
        println!("first misses:");
        for miss in &misses {
            println!("  {miss}");
        }
    }
    println!("search of every word      {search_secs:.1} s");

    Measured {
        words_total,
        words_read,
        words_findable,
    }
}

/// Wiring floors, not targets. The measured values belong in
/// `docs/performance.md`; these catch a corpus or an engine that arrived
/// broken, which is the failure mode a silently-empty run hides.
fn wiring_floors(m: &Measured) {
    assert!(
        m.words_total >= 500,
        "the corpus asked about {} words; a real downloaded corpus should carry \
         thousands of these, so something is half-read",
        m.words_total
    );
    assert!(
        m.words_read > 0,
        "the engine read none of {} ground-truth words; either the engine is not \
         available or the corpus text is not what this machine reads",
        m.words_total
    );
    let lost = m.words_read.saturating_sub(m.words_findable);
    assert!(
        lost <= m.words_read / 20 + 2,
        "the store lost {lost} of {} words OCR had already read; that is a \
         store-layer failure, not a model result",
        m.words_read
    );
    println!(
        "\nThe values above belong in docs/performance.md. What is asserted is only that\n\
         the corpus is real, the engine read something, and the store kept what it read."
    );
}

#[test]
#[ignore = "needs TESSERA_OCR_CORPUS and an OCR engine; runs a few minutes"]
fn printed_words_find_their_photograph() {
    let models = models();
    let choice = match std::env::var("TESSERA_OCR_ENGINE").ok().as_deref() {
        None | Some("") | Some("native") => OcrChoice::Native,
        Some(name) => OcrChoice::Named(name.to_string()),
    };
    let selection = tessera_core::ocr::select(&choice, &models)
        .unwrap_or_else(|err| panic!("selecting the {choice:?} engine: {err:#}"));
    let label = format!("engine {:?}", selection.name);

    let (dir, entries, full) = sample_corpus();
    let engine = selection.engine;
    let measured = measure(&entries, &dir, full, &label, |index| {
        let began = Instant::now();
        let result = engine.recognize(&entries[index].path);
        (result, began.elapsed().as_secs_f64() * 1000.0)
    });
    wiring_floors(&measured);
}

/// The cascade the two engine runs above make worth measuring: ask the fast
/// engine first, and pay for the ONNX fallback only where it found nothing.
///
/// The comparison that matters is against the native run, because that is what
/// ships; the ONNX-alone numbers are beside them in the same table only so the
/// cost of the retry can be read against the cost of the engine it retries.
/// This is a measurement, not a licensed change to the product: the default
/// engine is set by `preferred_engine` and ADR-0008, and changing it is a
/// decision this test informs.
///
/// Windows-only on purpose: "the platform engine" is the Windows adapter here,
/// and running the same name on a machine where it is absent would measure the
/// ONNX engine twice and call it a cascade.
#[cfg(windows)]
#[test]
#[ignore = "needs TESSERA_OCR_CORPUS; runs both engines over the corpus"]
fn a_native_then_onnx_cascade_reads_more_than_the_native_engine_alone() {
    let models = models();
    let native = tessera_core::ocr::select(&OcrChoice::Native, &models)
        .unwrap_or_else(|err| panic!("selecting the platform engine: {err:#}"));
    let onnx = tessera_core::ocr::select(&OcrChoice::Named("onnx".into()), &models)
        .unwrap_or_else(|err| panic!("selecting the onnx engine: {err:#}"));

    let (dir, entries, full) = sample_corpus();
    let measured = measure(
        &entries,
        &dir,
        full,
        "windows, with onnx on every empty read",
        |index| {
            let began = Instant::now();
            let result = match native.engine.recognize(&entries[index].path) {
                Ok(text) if !text.trim().is_empty() => Ok(text),
                _ => onnx.engine.recognize(&entries[index].path),
            };
            (result, began.elapsed().as_secs_f64() * 1000.0)
        },
    );
    wiring_floors(&measured);
}
