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
//! TESSERA_CORPUS_MANIFEST -> an alternate manifest in that directory
//! ```
//!
//! The manifest is a JSON array of `{ "path": ..., "id": ..., "captions": [...] }`
//! with `path` relative to that directory. The same directory carries two
//! manifests: `manifest.json` is the 300-photograph sample every number in
//! `docs/performance.md` was measured on, and the full Karpathy val split (5000
//! photographs) lives beside it, because the scaling test below wants the whole
//! shelf while the baseline tests want the sample. Point the scaling test at the
//! full set with `TESSERA_CORPUS_MANIFEST=manifest-5000.json`; the other tests
//! keep reading `manifest.json` and keep measuring what their numbers say they
//! measure.
//!
//! ```
//! $env:TESSERA_CORPUS="$env:LOCALAPPDATA\Temp\opencode\coco-eval"
//! cargo test --release -p tessera-core --test coco_retrieval -- --ignored --nocapture
//! ```
//!
//! Roughly three minutes: 300 image passes at ~240 ms, then one text pass per
//! caption at ~60 ms.
//!
//! Queries go through `embed_query` rather than `embed_text`, because that is the
//! path a search box takes: a sentence query is lower-cased and its trailing
//! period dropped before it is embedded. On this corpus that transform is worth
//! about four and a half points of R@1 in the measurement harness (0.6629 to
//! 0.7075), so a test that embedded the captions raw would be reporting a
//! configuration no user can reach. The raw figure stays in `docs/performance.md`
//! as what the tower does with text exactly as given.
//!
//! The second test in this file measures a query-side calibration on the same
//! embeds: a mean vector subtracted from each query before ranking, fitted on one
//! half of the corpus and scored on the other. It is measured rather than shipped,
//! and its own doc comment says why it is here.
//!
//! The third asks the first question again at six library sizes, because recall
//! is not a property of the model alone: it is the model against N candidates,
//! and the same configuration measured 0.78 at 150 candidates and 0.70 at 300.
//! A number quoted without its library size is not a number anyone can act on,
//! so this test grows the corpus from 150 photographs to all of it and prints the
//! whole curve. Its 300-candidate row is the corpus of the first test, so it
//! reproduces that headline exactly; its 150-candidate row is the size the
//! Python harness once measured in, and lands near that number with the same
//! small harness offset the paragraphs above already describe.

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
///
/// The file is `manifest.json` in the corpus directory, or the file named by
/// `TESSERA_CORPUS_MANIFEST`. That second name exists because the same
/// directory can carry a sample and the whole set: the baseline tests and the
/// numbers in `docs/performance.md` are taken on the sample, and the scaling
/// test wants every photograph that is on disk. Two manifests, one image
/// folder, and no run that silently measures the wrong one.
fn read_manifest(dir: &Path) -> Vec<Entry> {
    let manifest_path = dir.join(
        std::env::var("TESSERA_CORPUS_MANIFEST").unwrap_or_else(|_| "manifest.json".to_string()),
    );
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
        // The product's query path, not the tower in isolation. See the module
        // doc: the two differ by about four points of R@1 on sentence-shaped
        // queries, which is what these captions are.
        let query = embedder
            .embed_query(caption, DIM)
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

/// The library sizes the scaling test walks. The first two are the sizes
/// earlier measurements were taken at: the 300-candidate row shares its corpus
/// and code path with the first test in this file, so it must print that test's
/// headline, and 150 is the smaller shelf `docs/performance.md` records a number
/// for. The rest double until the corpus runs out.
const LADDER: [usize; 6] = [150, 300, 600, 1200, 2400, 5000];

/// Recall as the library grows: the same model, the same queries, the same
/// code, only N changes.
///
/// Embedding is done once; the curve is six rankings over subsets of the same
/// vectors. Queries for each row are the captions of the photographs *in* that
/// row's subset, so every query has its target among the candidates and the
/// chance rate is exactly 1/N. Subsets are even-stride samples of the whole
/// corpus, the formula the 300-photograph corpus was drawn with - so the
/// 300-candidate row is exactly the first test's corpus and code path, and must
/// print that test's headline. If it does not, one of the two changed without
/// its number changing, which is the failure this shared formula would catch.
///
/// Nothing here is asserted except the floor on the smallest row. The curve is
/// the measurement; an assertion on its shape would turn a finding into a claim
/// the moment it passed.
///
/// ```
/// $env:TESSERA_CORPUS="$env:LOCALAPPDATA\Temp\opencode\coco-eval"
/// $env:TESSERA_CORPUS_MANIFEST="manifest-5000.json"
/// cargo test --release -p tessera-core --test coco_retrieval recall_is_measured \
///   -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs the siglip model set and a corpus in TESSERA_CORPUS"]
fn recall_is_measured_at_every_library_size_of_the_corpus() {
    let models = models();
    assert!(
        SiglipEmbedder::is_installed(&models),
        "the siglip set is not installed; run: tessera models install --set siglip"
    );
    let dir = corpus_dir();
    let entries = read_manifest(&dir);
    let n = entries.len();
    assert!(
        n >= 8,
        "a scaling curve over {n} photographs would measure the sample, not the scale"
    );

    let embedder = SiglipEmbedder::new(&models, 4).expect("the embedder loads");

    let started = std::time::Instant::now();
    let images: Vec<Embedding> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            if index > 0 && index % 500 == 0 {
                eprintln!(
                    "  images {index}/{n} ({:.0} s)",
                    started.elapsed().as_secs_f64()
                );
            }
            embedder
                .embed_image(&entry.path, DIM)
                .unwrap_or_else(|err| panic!("embedding {}: {err}", entry.path.display()))
        })
        .collect();
    let indexing = started.elapsed();

    let query_list: Vec<(usize, &str)> = entries
        .iter()
        .enumerate()
        .flat_map(|(i, entry)| entry.captions.iter().map(move |c| (i, c.as_str())))
        .collect();
    let started = std::time::Instant::now();
    let mut captions: Vec<(usize, Embedding)> = Vec::with_capacity(query_list.len());
    for (index, (i, caption)) in query_list.iter().enumerate() {
        if index > 0 && index % 2000 == 0 {
            eprintln!(
                "  queries {index}/{} ({:.0} s)",
                query_list.len(),
                started.elapsed().as_secs_f64()
            );
        }
        let query = embedder
            .embed_query(caption, DIM)
            .unwrap_or_else(|err| panic!("embedding {caption:?}: {err}"));
        captions.push((*i, query));
    }
    let querying = started.elapsed();

    let mut ladder: Vec<usize> = LADDER.iter().copied().filter(|k| *k <= n).collect();
    if ladder.last() != Some(&n) {
        ladder.push(n);
    }

    println!(
        "\n== recall against a growing library: {n} photographs, {} captions ==",
        captions.len()
    );
    println!(
        "embedded: images {:.0} s, queries {:.0} s\n",
        indexing.as_secs_f64(),
        querying.as_secs_f64()
    );
    println!(
        "{:>6} {:>7} {:>7} {:>7} {:>7} {:>8} {:>10} {:>9}",
        "images", "queries", "R@1", "R@5", "R@10", "chance", "mean rank", "R@1 delta"
    );

    let mut first_row: Option<(f64, f64)> = None;
    let mut previous_r1: Option<f64> = None;
    for k in ladder {
        // Even-stride subset, inclusive of both ends; identity when k == n.
        let picked: Vec<usize> = (0..k).map(|i| i * (n - 1) / (k - 1)).collect();
        let queries: Vec<(usize, &Embedding)> = captions
            .iter()
            .filter(|(owner, _)| picked.binary_search(owner).is_ok())
            .map(|(owner, query)| (*owner, query))
            .collect();

        let scoring = std::time::Instant::now();
        let mut hits = [0usize; CUTOFFS.len()];
        let mut ranks: Vec<usize> = Vec::with_capacity(queries.len());
        for (owner, query) in &queries {
            let target = cosine(query, &images[*owner]);
            let ahead = picked
                .iter()
                .filter(|candidate| {
                    **candidate != *owner && cosine(query, &images[**candidate]) > target
                })
                .count();
            let rank = ahead + 1;
            ranks.push(rank);
            for (slot, cutoff) in CUTOFFS.iter().enumerate() {
                hits[slot] += usize::from(rank <= *cutoff);
            }
        }
        let shared = scoring.elapsed();

        let total = queries.len();
        let recall = |slot: usize| hits[slot] as f64 / total as f64;
        let mean_rank = ranks.iter().sum::<usize>() as f64 / ranks.len() as f64;
        let delta = previous_r1
            .map(|previous| format!("{:+.1}", (recall(0) - previous) * 100.0))
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{k:>6} {total:>7} {:>7.4} {:>7.4} {:>7.4} {:>8.4} {mean_rank:>10.1} {delta:>9}",
            recall(0),
            recall(1),
            recall(2),
            1.0 / k as f64
        );
        eprintln!("  ({k} candidates scored in {:.0} s)", shared.as_secs_f64());

        if first_row.is_none() {
            first_row = Some((recall(0), 1.0 / k as f64));
        }
        previous_r1 = Some(recall(0));
    }

    let (smallest_r1, smallest_chance) = first_row.expect("at least one row");
    assert!(
        smallest_r1 > 20.0 * smallest_chance,
        "the smallest row's R@1 {smallest_r1:.4} is not far enough above its {smallest_chance:.4} \
         chance rate; either the corpus or the model wiring is wrong"
    );
    println!(
        "\nThe curve belongs in docs/performance.md. What is asserted is only that the\n\
         smallest library retrieves far above chance - a wiring check, not a target."
    );
}

struct Scored {
    label: String,
    images: Vec<Embedding>,
    /// `(the photograph this query belongs to, the query vector)`.
    queries: Vec<(usize, Embedding)>,
}

/// Unit-length copy of a vector, for the arithmetic below.
fn as_unit(values: Vec<f32>) -> Embedding {
    Embedding::new(values, DIM)
        .expect("a DIM-wide vector")
        .normalized()
}

/// The mean of a set of embeddings. Its length is not used: what gets subtracted
/// is a direction, and the caller re-normalises.
fn mean<'a>(vectors: impl Iterator<Item = &'a Embedding>) -> Embedding {
    let mut sum = vec![0.0f32; DIM];
    let mut count = 0usize;
    for vector in vectors {
        for (slot, value) in sum.iter_mut().zip(vector.values()) {
            *slot += value;
        }
        count += 1;
    }
    assert!(count > 0, "a mean over nothing would be a zero vector");
    for slot in &mut sum {
        *slot /= count as f32;
    }
    Embedding::new(sum, DIM).expect("a DIM-wide vector")
}

/// `vector - alpha * mean`, re-normalised. `alpha = 0.0` is a unit-length copy.
fn shifted(vector: &Embedding, mean: &Embedding, alpha: f32) -> Embedding {
    let values: Vec<f32> = vector
        .values()
        .iter()
        .zip(mean.values())
        .map(|(value, offset)| value - alpha * offset)
        .collect();
    as_unit(values)
}

/// Rank of the right photograph for every query, ties broken against us.
fn rank_of(queries: &[(usize, Embedding)], images: &[Embedding]) -> Vec<usize> {
    queries
        .iter()
        .map(|(target, query)| {
            let right = cosine(query, &images[*target]);
            let ahead = images
                .iter()
                .enumerate()
                .filter(|(index, image)| index != target && cosine(query, image) > right)
                .count();
            ahead + 1
        })
        .collect()
}

/// A query-side mean, fitted on one half of the corpus and scored on the other.
///
/// This measures a transform the engine does **not** ship. It is here because it
/// is the one query-time change measured on this corpus that gains points instead
/// of losing them, and because the number that would decide whether to ship it
/// should be re-derivable from this repository. The geometry, crop and whitening
/// tables in `docs/performance.md` came from a harness that lived in a temporary
/// directory and is gone; that is a gap worth not repeating on the piece of
/// evidence the next decision rests on.
///
/// The split is fixed - even image indices against odd, no seed to tune - and the
/// mean is fitted on the training half's captions and scored on the other half's,
/// with all the photographs as candidates. Both directions are printed: a gain in
/// one direction is a coin flip, a gain in both is a finding.
///
/// Only a mean is removed, and *which* mean decides the sign. Subtracting the
/// images' own mean from text vectors takes R@1 down by about fifteen points, for
/// the same reason whitening-on-images lands on chance: it moves the text
/// somewhere the images are not. Subtracting the captions' mean from the query
/// side alone gains, and that is the only version of this that needs no re-index.
///
/// Nothing here is asserted except the floors at the end. The measured values
/// belong in `docs/performance.md`; an assertion on the size of a gain would turn
/// a measurement into a claim the moment it passed.
#[test]
#[ignore = "needs the siglip model set and a corpus in TESSERA_CORPUS"]
fn a_query_side_mean_can_be_scored_on_a_held_out_half() {
    let models = models();
    assert!(
        SiglipEmbedder::is_installed(&models),
        "the siglip set is not installed; run: tessera models install --set siglip"
    );
    let dir = corpus_dir();
    let entries = read_manifest(&dir);
    let n = entries.len();
    assert!(
        n >= 8,
        "a held-out split over {n} photographs would measure the split, not the transform"
    );

    let embedder = SiglipEmbedder::new(&models, 4).expect("the embedder loads");
    let started = std::time::Instant::now();
    let images: Vec<Embedding> = entries
        .iter()
        .map(|entry| {
            embedder
                .embed_image(&entry.path, DIM)
                .unwrap_or_else(|err| panic!("embedding {}: {err}", entry.path.display()))
                .normalized()
        })
        .collect();
    let captions: Vec<(usize, Embedding)> = entries
        .iter()
        .enumerate()
        .flat_map(|(i, entry)| entry.captions.iter().map(move |caption| (i, caption)))
        .map(|(i, caption)| {
            let query = embedder
                .embed_query(caption, DIM)
                .unwrap_or_else(|err| panic!("embedding {caption:?}: {err}"));
            (i, query.normalized())
        })
        .collect();
    println!(
        "\n== query-side calibration: {n} photographs, {} captions, embedded in {:.0} s ==",
        captions.len(),
        started.elapsed().as_secs_f64()
    );
    let chance = 1.0 / n as f64;

    let mut floors: Vec<(&str, f64, f64)> = Vec::new();
    for (direction, train_is_even) in [
        ("even photographs to fit, odd to score", true),
        ("odd photographs to fit, even to score", false),
    ] {
        let in_train = |image: usize| image.is_multiple_of(2) == train_is_even;
        let train_images: Vec<usize> = (0..n).filter(|i| in_train(*i)).collect();
        let train_rows: Vec<usize> = (0..captions.len())
            .filter(|r| in_train(captions[*r].0))
            .collect();
        let test_rows: Vec<usize> = (0..captions.len())
            .filter(|r| !in_train(captions[*r].0))
            .collect();

        let mean_images = mean(train_images.iter().map(|i| &images[*i]));
        let mean_captions = mean(train_rows.iter().map(|r| &captions[*r].1));
        let mean_both = mean(
            train_images
                .iter()
                .map(|i| &images[*i])
                .chain(train_rows.iter().map(|r| &captions[*r].1)),
        );

        let images_shifted = |mean: &Embedding, alpha: f32| -> Vec<Embedding> {
            images
                .iter()
                .map(|image| shifted(image, mean, alpha))
                .collect()
        };
        let queries_shifted = |mean: &Embedding, alpha: f32| -> Vec<(usize, Embedding)> {
            test_rows
                .iter()
                .map(|r| (captions[*r].0, shifted(&captions[*r].1, mean, alpha)))
                .collect()
        };
        let queries_raw = || -> Vec<(usize, Embedding)> {
            test_rows
                .iter()
                .map(|r| (captions[*r].0, captions[*r].1.clone()))
                .collect()
        };

        let mut scored: Vec<Scored> = vec![
            Scored {
                label: "nothing - the shipped configuration".to_string(),
                images: images.clone(),
                queries: queries_raw(),
            },
            Scored {
                label: "the images' mean, both sides".to_string(),
                images: images_shifted(&mean_images, 1.0),
                queries: queries_shifted(&mean_images, 1.0),
            },
            Scored {
                label: "each side its own mean".to_string(),
                images: images_shifted(&mean_images, 1.0),
                queries: queries_shifted(&mean_captions, 1.0),
            },
            Scored {
                label: "the captions' mean, both sides".to_string(),
                images: images_shifted(&mean_captions, 1.0),
                queries: queries_shifted(&mean_captions, 1.0),
            },
            Scored {
                label: "the images' and captions' mean, both sides".to_string(),
                images: images_shifted(&mean_both, 1.0),
                queries: queries_shifted(&mean_both, 1.0),
            },
            Scored {
                label: "the captions' mean, query side only".to_string(),
                images: images.clone(),
                queries: queries_shifted(&mean_captions, 1.0),
            },
        ];
        for alpha in [0.25f32, 0.5, 0.75] {
            scored.push(Scored {
                label: format!("{alpha} of the captions' mean, query side only"),
                images: images.clone(),
                queries: queries_shifted(&mean_captions, alpha),
            });
        }

        let ranks: Vec<Vec<usize>> = scored
            .iter()
            .map(|entry| rank_of(&entry.queries, &entry.images))
            .collect();
        let share = |row: usize, cutoff: usize| -> f64 {
            ranks[row].iter().filter(|rank| **rank <= cutoff).count() as f64
                / ranks[row].len() as f64
        };

        println!(
            "\n-- {direction}: {} photographs and {} captions to fit, {} queries to score --",
            train_images.len(),
            train_rows.len(),
            test_rows.len()
        );
        println!(
            "{:<44} {:>6} {:>6} {:>7} {:>3} {:>4}",
            "variant", "R@1", "R@5", "delta", "in", "out"
        );
        for (row, entry) in scored.iter().enumerate() {
            if row == 0 {
                println!(
                    "{:<44} {:>6.4} {:>6.4}",
                    entry.label,
                    share(0, 1),
                    share(0, 5)
                );
                continue;
            }
            let moved_in = ranks[row]
                .iter()
                .zip(&ranks[0])
                .filter(|(new, old)| **new <= 1 && **old > 1)
                .count();
            let moved_out = ranks[row]
                .iter()
                .zip(&ranks[0])
                .filter(|(new, old)| **new > 1 && **old <= 1)
                .count();
            println!(
                "{:<44} {:>6.4} {:>6.4} {:>+6.1} {moved_in:>3} {moved_out:>4}",
                entry.label,
                share(row, 1),
                share(row, 5),
                (share(row, 1) - share(0, 1)) * 100.0
            );
        }
        // Row 5 is the query-side-only variant, by construction above.
        floors.push((direction, share(0, 1), share(5, 1)));
    }

    for (direction, raw, query_side) in &floors {
        assert!(
            *raw > 20.0 * chance,
            "{direction}: the baseline R@1 {raw:.4} is not far enough above the {chance:.4} \
             chance rate for {n} images; either the corpus or the model wiring is wrong"
        );
        assert!(
            *query_side > 10.0 * chance,
            "{direction}: the query-side mean leaves R@1 at {query_side:.4}, against a {chance:.4} \
             chance rate - a collapse that far is an arithmetic bug, not a result"
        );
    }
    println!(
        "\nThe values above belong in docs/performance.md, not in an assertion here. What is\n\
         asserted is only that both configurations retrieve at all."
    );
}
