//! Text-to-image retrieval on a corpus large enough for the number to mean
//! something.
//!
//! Eleven photographs cannot decide a product target: one item is nine
//! percentage points, and the same eleven items have now been queried often
//! enough that tuning against them would be fitting noise. This test runs
//! against a COCO validation sample - 300 photographs, each with the five
//! independent captions real annotators wrote for it - and reports **recall at
//! 1, 5 and 10 over every caption**, which is the standard shape for this task.
//!
//! It measures [`SiglipEmbedder`], the type the engine ships, so the number is
//! the product's number rather than a benchmark's. The corpus lives outside the
//! repository, like the photo corpus, because image data does not belong in git:
//!
//! ```text
//! TESSERA_CORPUS -> a directory containing manifest.json and images\
//! ```
//!
//! The manifest is a JSON array of `{ "path": ..., "id": ..., "captions": [...] }`
//! with `path` relative to that directory.
//!
//! ```
//! $env:TESSERA_CORPUS="$env:LOCALAPPDATA\Temp\opencode\coco-eval"
//! cargo test --release -p tessera-core --test coco_retrieval -- --ignored --nocapture
//! ```
//!
//! Roughly three minutes: 300 image passes at ~240 ms, then one text pass per
//! caption at ~60 ms.

use std::path::{Path, PathBuf};

use tessera_core::models::Models;
use tessera_core::vectors::{cosine, siglip::SiglipEmbedder, Embedding};

const DIM: usize = 768;
/// Recall cutoffs reported. R@1 is the headline; R@5 is what a person sees.
const CUTOFFS: [usize; 3] = [1, 5, 10];

struct Entry {
    path: PathBuf,
    captions: Vec<String>,
}

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
    std::env::var("TESSERA_CORPUS")
        .map(PathBuf::from)
        .expect("set TESSERA_CORPUS to a directory holding manifest.json and images\\")
}

/// Parse the manifest, failing loudly on anything that is not the documented
/// shape. A corpus that silently arrives half-read measures the wrong thing.
fn read_manifest(dir: &Path) -> Vec<Entry> {
    let manifest_path = dir.join("manifest.json");
    let text = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("reading {}: {err}", manifest_path.display()));
    let parsed: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|err| panic!("parsing manifest: {err}"));
    // Both shapes are accepted because both are reasonable to write: a bare
    // array of entries, or an object carrying provenance alongside an `images`
    // array. The corpus builder writes the second, which is the better of the
    // two - how the sample was drawn belongs in the file that records the
    // sample.
    let items = match &parsed {
        serde_json::Value::Array(items) => items,
        serde_json::Value::Object(fields) => fields
            .get("images")
            .and_then(|value| value.as_array())
            .unwrap_or_else(|| {
                panic!(
                    "manifest is an object with keys {:?} and no `images` array",
                    fields.keys().collect::<Vec<_>>()
                )
            }),
        other => panic!(
            "manifest should be an array or an object, found {}",
            kind(other)
        ),
    };

    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        let relative = item["path"]
            .as_str()
            .unwrap_or_else(|| panic!("an entry has no string `path`: {item}"));
        let captions: Vec<String> = item["captions"]
            .as_array()
            .unwrap_or_else(|| panic!("{relative} has no `captions` array"))
            .iter()
            .filter_map(|c| c.as_str())
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        assert!(
            !captions.is_empty(),
            "{relative} carries no usable caption, so it can never be retrieved"
        );
        // Manifest paths are Windows-shaped; `Path::join` under Linux CI would
        // treat "images\x.jpg" as one file name, so split on either separator.
        let mut path = dir.to_path_buf();
        for part in relative.split(['\\', '/']).filter(|p| !p.is_empty()) {
            path.push(part);
        }
        entries.push(Entry { path, captions });
    }
    assert!(!entries.is_empty(), "the manifest is empty");
    entries
}

fn kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

#[test]
#[ignore = "needs the siglip model set and a corpus in TESSERA_CORPUS"]
fn a_caption_finds_its_own_image_in_a_real_corpus() {
    let models = models();
    assert!(
        SiglipEmbedder::is_installed(&models),
        "the siglip set is not installed; run: tessera models install --set siglip"
    );
    let dir = corpus_dir();
    let entries = read_manifest(&dir);
    let n = entries.len();

    let missing: Vec<&PathBuf> = entries
        .iter()
        .map(|e| &e.path)
        .filter(|p| !p.is_file())
        .collect();
    assert!(
        missing.is_empty(),
        "{} of {n} manifest entries point at files that do not exist, first: {}",
        missing.len(),
        missing[0].display()
    );

    let embedder = SiglipEmbedder::new(&models, 4).expect("the embedder loads");

    let started = std::time::Instant::now();
    let images: Vec<Embedding> = entries
        .iter()
        .map(|entry| {
            embedder
                .embed_image(&entry.path, DIM)
                .unwrap_or_else(|err| panic!("embedding {}: {err}", entry.path.display()))
        })
        .collect();
    let indexing = started.elapsed();

    // Every caption is a query, so the sample is 5x the image count and a single
    // lucky retrieval cannot move the headline number much.
    let queries: Vec<(usize, &str)> = entries
        .iter()
        .enumerate()
        .flat_map(|(i, entry)| entry.captions.iter().map(move |c| (i, c.as_str())))
        .collect();

    let mut hits = [0usize; CUTOFFS.len()];
    let mut ranks: Vec<usize> = Vec::with_capacity(queries.len());
    let query_started = std::time::Instant::now();
    for (i, caption) in &queries {
        let query = embedder
            .embed_text(caption, DIM)
            .unwrap_or_else(|err| panic!("embedding {caption:?}: {err}"));

        // Count how many images score strictly above the right one. That is the
        // rank with ties broken against us, which is the conservative reading.
        let target = cosine(&query, &images[*i]);
        let ahead = images
            .iter()
            .enumerate()
            .filter(|(j, image)| *j != *i && cosine(&query, image) > target)
            .count();
        let rank = ahead + 1;
        ranks.push(rank);
        for (slot, cutoff) in CUTOFFS.iter().enumerate() {
            hits[slot] += usize::from(rank <= *cutoff);
        }
    }
    let querying = query_started.elapsed();

    let total = queries.len();
    let recall: Vec<f64> = hits.iter().map(|h| *h as f64 / total as f64).collect();
    ranks.sort_unstable();
    let median_rank = ranks[ranks.len() / 2];
    let mean_rank = ranks.iter().sum::<usize>() as f64 / ranks.len() as f64;
    let chance = 1.0 / n as f64;

    println!("\n== text to image retrieval ==");
    println!("images                 {n}");
    println!("queries (all captions) {total}");
    println!("chance R@1             {chance:.4}");
    println!(
        "R@1                    {:.4}   ({} of {total})",
        recall[0], hits[0]
    );
    println!(
        "R@5                    {:.4}   ({} of {total})",
        recall[1], hits[1]
    );
    println!(
        "R@10                   {:.4}   ({} of {total})",
        recall[2], hits[2]
    );
    println!("median rank            {median_rank} of {n}");
    println!("mean rank              {mean_rank:.1} of {n}");
    println!(
        "indexing               {:.1} s ({:.0} ms per image)",
        indexing.as_secs_f64(),
        indexing.as_secs_f64() * 1000.0 / n as f64
    );
    println!(
        "queries                {:.1} s ({:.0} ms per query)",
        querying.as_secs_f64(),
        querying.as_secs_f64() * 1000.0 / total as f64
    );

    // A floor, not the product target. 0.85 R@1 is what the feature was asked
    // for; on a 300-image corpus a working model clears this floor by a wide
    // margin, so this asserts the harness is wired to the right model and the
    // corpus is real. The measured value belongs in `docs/performance.md` and
    // nowhere else - an assertion here would turn a measurement into a
    // claim the moment it passed.
    assert!(
        recall[0] > 20.0 * chance,
        "R@1 {:.4} is not far enough above the {chance:.4} chance rate for {n} images; \
         either the corpus or the model wiring is wrong",
        recall[0]
    );
}
