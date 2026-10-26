//! The image arm's synthetic rendered-text control (ticket 07).
//!
//! The week-1 spike measured top-1 0.90 on 20 rendered-text images and that
//! number was quoted in `docs/performance.md` long before anything in this
//! repository could reproduce it. This test is that control, run through the
//! shipping embedder on the committed fixtures in `fixtures/rendered/`.
//!
//! What it measures: given an image whose entire content is a rendered phrase,
//! does description search put that image first for its own phrase? It is a
//! control, not a benchmark — the images are synthetic, deliberately unlike
//! each other, and the phrases are the labels. A number here says the embedding
//! space can tell rendered text apart; it says nothing about photographs.
//!
//! ```
//! cargo test --release -p tessera-core --test rendered_control -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use serde::Deserialize;
use tessera_core::models::Models;
use tessera_core::vectors::siglip::SiglipEmbedder;
use tessera_core::vectors::{cosine, Embedding};

const DIM: usize = 768;

#[derive(Deserialize)]
struct Manifest {
    images: Vec<Control>,
}

#[derive(Deserialize)]
struct Control {
    file: String,
    phrase: String,
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

#[test]
#[ignore = "needs the siglip model set"]
fn rendered_text_is_found_by_its_own_phrase() {
    let models = models();
    assert!(
        SiglipEmbedder::is_installed(&models),
        "install the siglip set: tessera models install --set siglip"
    );
    let embedder = SiglipEmbedder::new(&models, 4).expect("embedder");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rendered");
    let manifest: Manifest =
        serde_json::from_str(include_str!("fixtures/rendered/control.json")).expect("manifest");

    let mut images: Vec<(Embedding, &str)> = Vec::new();
    for control in &manifest.images {
        let path = root.join(&control.file);
        let embedding = embedder
            .embed_image(&path, DIM)
            .unwrap_or_else(|err| panic!("embedding {}: {err:#}", path.display()));
        images.push((embedding, control.phrase.as_str()));
    }

    let mut hits = 0;
    let mut rank_sum = 0;
    for (index, (_, phrase)) in images.iter().enumerate() {
        let query = embedder
            .embed_text(phrase, DIM)
            .unwrap_or_else(|err| panic!("embedding {phrase:?}: {err:#}"));
        let mut scored: Vec<(f32, usize)> = images
            .iter()
            .enumerate()
            .map(|(candidate, (image, _))| (cosine(&query, image), candidate))
            .collect();
        // Strictly-higher ties beat the right answer, as in the other recall
        // harnesses: a tie is not a success.
        let score = scored[index].0;
        let rank = 1 + scored.iter().filter(|(other, _)| *other > score).count();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let top = scored[0].1;
        let ok = top == index;
        hits += usize::from(ok);
        rank_sum += rank;
        eprintln!(
            "  {} rank {rank:>2}  {phrase}",
            if ok { "HIT " } else { "miss" }
        );
    }

    let total = images.len();
    let top1 = hits as f64 / total as f64;
    let mean_rank = rank_sum as f64 / total as f64;
    eprintln!(
        "\nrendered-text control: top-1 {hits}/{total} = {top1:.2}, mean rank {mean_rank:.2}, \
         chance {:.3}",
        1.0 / total as f64
    );
    assert!(
        top1 >= 0.90,
        "the image arm fell to {top1:.2} on rendered text against the 0.90 control; \
         docs/performance.md says this control is met, so a run below it is a regression"
    );
}
