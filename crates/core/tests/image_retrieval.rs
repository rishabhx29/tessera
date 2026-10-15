//! Can a description find the photo it describes?
//!
//! The acceptance measurement for search-by-description, run through the type
//! the engine uses ([`SiglipEmbedder`]) against the pinned model set. It
//! replaces an earlier version of this test that loaded the combined
//! `model_uint8.onnx` export directly, on the belief that the per-tower exports
//! lacked projection heads. They do not: on identical inputs and the same
//! corpus, both exports score the same top-1, and only the combined export costs
//! a vision-tower pass on every text query (`docs/performance.md`).
//!
//! The corpus is a folder of JPEGs whose file names are the queries, so the
//! measurement is only as good as those names: they are short, descriptive and
//! written by whoever saved the file, which makes this a **hard** corpus rather
//! than a representative one. Four of the eleven photos are dogs, and three of
//! those are dogs on grass, so most of the ranking work here is attribute
//! binding — the thing single-vector embedding models are worst at. A number
//! from this test is a floor and a regression check, not a product claim.
//!
//! ```
//! $env:TESSERA_PHOTOS="$env:LOCALAPPDATA\Temp\opencode\photos"
//! cargo test --release -p tessera-core --test image_retrieval -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};

use tessera_core::models::Models;
use tessera_core::vectors::{cosine, siglip::SiglipEmbedder, Embedding};

const DIM: usize = 768;

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

fn photo_dir() -> PathBuf {
    std::env::var("TESSERA_PHOTOS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            PathBuf::from(home).join("photos")
        })
}

fn is_image(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("jpg" | "jpeg" | "png")
    )
}

/// The query for a photo: its file name, with separators opened out into words.
fn label_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().replace(['-', '_'], " "))
        .unwrap_or_default()
}

#[test]
#[ignore = "needs the siglip model set and a labelled photo corpus"]
fn a_description_of_a_photo_finds_that_photo() {
    let models = models();
    assert!(
        SiglipEmbedder::is_installed(&models),
        "the siglip set is not installed; run: tessera models install --set siglip"
    );
    let folder = photo_dir();
    assert!(folder.is_dir(), "no photo folder at {}", folder.display());

    let mut paths: Vec<PathBuf> = std::fs::read_dir(&folder)
        .expect("reading the photo folder")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_image(path))
        .collect();
    paths.sort();
    let n = paths.len();
    assert!(n >= 5, "{n} photos is too few to mean anything");
    let chance = 1.0 / n as f64;

    let embedder = SiglipEmbedder::new(&models, 4).expect("the embedder loads");
    let images: Vec<Embedding> = paths
        .iter()
        .map(|path| {
            embedder
                .embed_image(path, DIM)
                .unwrap_or_else(|err| panic!("embedding {}: {err}", path.display()))
        })
        .collect();

    let mut top1 = 0usize;
    let mut top3 = 0usize;
    for (i, path) in paths.iter().enumerate() {
        let label = label_of(path);
        let query = embedder
            .embed_text(&label, DIM)
            .unwrap_or_else(|err| panic!("embedding {label:?}: {err}"));

        let mut ranked: Vec<(usize, f32)> = images
            .iter()
            .enumerate()
            .map(|(j, image)| (j, cosine(&query, image)))
            .collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).expect("no NaN cosine"));

        let best = ranked[0].0;
        let in_top3 = ranked[..3.min(n)].iter().any(|(j, _)| *j == i);
        top1 += usize::from(best == i);
        top3 += usize::from(in_top3);
        println!(
            "  {} {label:<42} -> {:<42} {:.3}{}",
            if best == i { "HIT " } else { "miss" },
            label_of(&paths[best]),
            ranked[0].1,
            if in_top3 && best != i {
                "  (in top 3)"
            } else {
                ""
            }
        );
    }

    let top1_rate = top1 as f64 / n as f64;
    let top3_rate = top3 as f64 / n as f64;
    println!("\n== search by description ==");
    println!("photos        {n}   ({top1} top-1, {top3} top-3)");
    println!("chance top-1  {chance:.3}");
    println!("top-1         {top1_rate:.3}");
    println!("top-3         {top3_rate:.3}");

    // Above chance, and above chance by a margin that a lucky corpus could not
    // produce. The 85% bar for this feature is a product target, not something
    // eleven photos can decide; `docs/performance.md` records what is actually
    // measured and what is not.
    assert!(
        top1_rate > 1.5 * chance,
        "top-1 {top1_rate:.3} is barely above the {chance:.3} chance rate for {n} photos"
    );
}
