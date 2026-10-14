//! Diagnostic: what do the two SigLIP towers actually take and return?
//!
//! Written because the shipped export did not match what the code expected —
//! the vision tower returns per-patch features, not a pooled vector, and the
//! text tower does not take the two inputs the code guessed. Guessing from the
//! model card is how that happened; reading the graph is how it gets fixed.

use std::sync::OnceLock;

fn models() -> tessera_core::models::Models {
    let dir = std::env::var("TESSERA_MODELS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            tessera_core::cli::models_dir(
                &tessera_core::cli::default_data_dir().expect("a data dir"),
                None,
            )
        });
    tessera_core::models::Models::open(&dir).expect("models dir")
}

#[test]
#[ignore = "diagnostic; needs the siglip model set"]
fn dump_the_signatures() {
    static ENV: OnceLock<()> = OnceLock::new();
    let _ = ENV.get_or_init(|| {});
    let environment = ort::init().build().expect("environment");
    let models = models();

    for (label, file) in [
        ("vision", "siglip-vision.int8.onnx"),
        ("text", "siglip-text.int8.onnx"),
    ] {
        let path = models
            .verified_path("siglip", file)
            .expect("the model is installed");
        let session = ort::session::Session::builder(&environment)
            .expect("builder")
            .commit_from_file(&path)
            .expect("load");

        println!("\n=== {label} ({}) ===", path.display());
        println!("inputs:");
        for (i, input) in session.inputs().iter().enumerate() {
            println!("  [{i}] {:?}", input.name());
        }
        println!("outputs:");
        for (i, output) in session.outputs().iter().enumerate() {
            println!("  [{i}] {:?}", output.name());
        }
    }
}
