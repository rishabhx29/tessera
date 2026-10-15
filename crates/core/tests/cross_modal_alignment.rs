//! Does the shipped configuration put text and images in one space?
//!
//! This is the check that would have prevented a long detour, so it is worth
//! stating plainly what it does and does not measure.
//!
//! **Same-modality similarity is not a defect.** Raw transformer embeddings are
//! anisotropic: they share a large common direction, so two unrelated sentences
//! sit near 0.8 and two solid-colour images near 0.9, in a perfectly healthy
//! model. An earlier investigation read those numbers as a fault, concluded the
//! towers needed projection heads the export did not contain, and adopted a
//! second 205 MB model to supply them. Measured on identical inputs the two
//! exports produce the same vectors (cosine 0.996) and the same retrieval
//! (`docs/performance.md`), so the adoption bought nothing.
//!
//! **Cross-modal similarity is.** Once two towers share a space, an image and a
//! sentence that do not describe each other must sit near zero. That is the
//! property that fails when the two sides are read inconsistently, and it is
//! what this test asserts.
//!
//! It runs against [`SiglipEmbedder`], the type the engine actually uses, on the
//! pinned model set — not against a separate benchmark export. A test that loads
//! its own copy of a model can pass while the shipped path is broken.
//!
//! ```
//! cargo test --release -p tessera-core --test cross_modal_alignment -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use image::{ImageBuffer, Rgb};
use tessera_core::models::Models;
use tessera_core::vectors::siglip::SiglipEmbedder;
use tessera_core::vectors::{cosine, Embedding};

/// SigLIP's shared space is 768-dimensional.
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

/// A solid grey field, written where the engine's own loader can read it.
///
/// Generated rather than committed: a fixture that is a flat colour is the one
/// thing that needs no provenance, and the repository already refuses to carry
/// image data it cannot account for.
fn solid(path: &std::path::Path, grey: u8) {
    let mut image: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(64, 64);
    for pixel in image.pixels_mut() {
        *pixel = Rgb([grey, grey, grey]);
    }
    image.save(path).expect("writing a solid image");
}

#[test]
#[ignore = "needs the siglip model set"]
fn unrelated_text_and_image_sit_near_zero() {
    let models = models();
    assert!(
        SiglipEmbedder::is_installed(&models),
        "the siglip set is not installed; run: tessera models install --set siglip"
    );
    let embedder = SiglipEmbedder::new(&models, 4).expect("the embedder loads");

    let dir = tempfile::tempdir().expect("a temp dir");
    let black_path = dir.path().join("black.png");
    let white_path = dir.path().join("white.png");
    solid(&black_path, 0);
    solid(&white_path, 255);

    let black: Embedding = embedder.embed_image(&black_path, DIM).expect("black");
    let white: Embedding = embedder.embed_image(&white_path, DIM).expect("white");
    let dog: Embedding = embedder.embed_text("a dog", DIM).expect("'a dog'");
    let ocean: Embedding = embedder
        .embed_text("the ocean at sunset", DIM)
        .expect("ocean");

    println!("\n== the four pairs ==");
    let same_text = cosine(&dog, &ocean);
    let same_image = cosine(&black, &white);
    let crossed_a = cosine(&dog, &black);
    let crossed_b = cosine(&ocean, &white);
    println!("  text  'a dog' vs 'the ocean at sunset' = {same_text:+.3}");
    println!("  image black   vs white                 = {same_image:+.3}");
    println!("  cross 'a dog' vs black                 = {crossed_a:+.3}");
    println!("  cross 'the ocean at sunset' vs white   = {crossed_b:+.3}");

    // Asserted, and the assertion is that these stay high. Two unrelated
    // sentences sharing a direction is what anisotropy *is*; a run where this
    // drops toward zero would mean the text tower had stopped producing
    // sentence-like vectors at all.
    assert!(
        same_text > 0.5,
        "unrelated sentences sit at {same_text:+.3}. Raw transformer embeddings are anisotropic \
         and should share a large common direction here; a value this low means the text tower is \
         no longer encoding sentences, not that it has become more discriminative."
    );
    assert!(
        same_image > 0.5,
        "black and white sit at {same_image:+.3}, which is far below the ~0.9 a healthy vision \
         tower gives for two flat fields."
    );

    // The property that carries meaning.
    for (label, value) in [
        ("'a dog' vs black", crossed_a),
        ("'the ocean at sunset' vs white", crossed_b),
    ] {
        assert!(
            value.abs() < 0.3,
            "cross-modal {label} is {value:+.3}; an image and a sentence that do not describe each \
             other must sit near zero when both towers share a space. A value near the \
             same-modality figures above means the two sides are being read differently."
        );
    }
}
