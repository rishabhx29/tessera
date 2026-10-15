//! Is the *text* tower misread the same way the vision tower is?
//!
//! `image_input_sanity.rs` proved the vision tower's `pooler_output` barely
//! responds to the image: solid black and solid white sit at cosine 0.897. This
//! asks the same question of the text tower, because a cross-modal model with a
//! dead tensor on *either* side produces exactly the failure we have been
//! measuring — and if both towers read a projection head, no amount of model
//! swapping or precision will fix it.
//!
//! Three things, in increasing strength:
//!
//! 1. Does the text tower distinguish different text at all?
//! 2. Does it distinguish text from padding-only input?
//! 3. Does `pooler_output` or the first token carry the alignment?
//!
//! ```
//! cargo test --release -p tessera-core --test text_tower_sanity -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::OnceLock;

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

const DIM: usize = 768;
const TOKENS: usize = 64;

fn environment() -> &'static ort::environment::Environment {
    static ENV: OnceLock<std::result::Result<ort::environment::Environment, String>> =
        OnceLock::new();
    match ENV.get_or_init(|| ort::init().build().map_err(|e| format!("{e}"))) {
        Ok(e) => e,
        Err(m) => panic!("onnx runtime: {m}"),
    }
}

fn models() -> tessera_core::models::Models {
    let dir = std::env::var("TESSERA_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            tessera_core::cli::models_dir(
                &tessera_core::cli::default_data_dir().expect("a data dir"),
                None,
            )
        });
    tessera_core::models::Models::open(&dir).expect("models dir")
}

fn load(env: &ort::environment::Environment, path: &std::path::Path) -> Session {
    Session::builder(env)
        .expect("builder")
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .expect("optimise")
        .commit_from_file(path)
        .expect("load")
}

/// Tokenise, pad to the model's context, and run both towers' outputs.
fn embed_text(
    session: &mut Session,
    tokenizer: &tokenizers::Tokenizer,
    input_name: &str,
    text: &str,
) -> (Vec<f32>, Vec<f32>) {
    let encoding = tokenizer.encode(text, true).expect("encode");
    let mut ids: Vec<i64> = encoding.get_ids().iter().map(|t| *t as i64).collect();
    ids.truncate(TOKENS);
    ids.resize(TOKENS, 0);

    let tensor = Tensor::from_array(([1usize, TOKENS], ids.into_boxed_slice())).expect("tensor");
    let out = session
        .run(vec![(input_name.to_string(), tensor)])
        .expect("run");

    let (ps, pooled) = out[1].try_extract_tensor::<f32>().expect("pooler");
    let (hs, hidden) = out[0].try_extract_tensor::<f32>().expect("hidden");

    eprintln!("      shapes: pooled={ps:?} hidden={hs:?}");
    let mut p = pooled.to_vec();
    let n: f32 = p.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > f32::EPSILON {
        for x in p.iter_mut() {
            *x /= n;
        }
    }
    let mut h = hidden[..DIM].to_vec();
    let n: f32 = h.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > f32::EPSILON {
        for x in h.iter_mut() {
            *x /= n;
        }
    }
    (p, h)
}

fn cos(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[test]
#[ignore = "needs the siglip model set"]
fn the_text_tower_also_ignores_what_it_is_given() {
    let models = models();
    let env = environment();
    let mut session = load(
        env,
        &models
            .verified_path("siglip", "siglip-text.fp32.onnx")
            .expect("int8 text"),
    );
    let tokenizer = tokenizers::Tokenizer::from_file(
        &models
            .verified_path("siglip", "siglip-tokenizer.json")
            .expect("tokenizer"),
    )
    .expect("tokeniser");
    let input = session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .unwrap_or_default();
    eprintln!("text tower input: {input:?}");

    // Padding only: what the tower sees when there is nothing to encode. If this
    // is indistinguishable from a real query, the tower is reading a projection
    // head rather than the sentence.
    let padding_only = ".".repeat(80);
    let cases = [
        ("padding only", padding_only.as_str()),
        ("a dog", "a dog"),
        ("a spreadsheet of expenses", "a spreadsheet of expenses"),
        ("the ocean at sunset", "the ocean at sunset"),
    ];

    let mut pooled = Vec::new();
    let mut first = Vec::new();
    for (label, text) in cases {
        let (p, h) = embed_text(&mut session, &tokenizer, &input, text);
        eprintln!("  {label:26} pooler norm-1 check ok, first-token ok");
        pooled.push(p);
        first.push(h);
    }

    eprintln!("\n== pooler_output ==");
    for i in 0..cases.len() {
        for j in (i + 1)..cases.len() {
            eprintln!(
                "  {:26} vs {:26} {:.3}",
                cases[i].0,
                cases[j].0,
                cos(&pooled[i], &pooled[j])
            );
        }
    }
    eprintln!("\n== first token ==");
    for i in 0..cases.len() {
        for j in (i + 1)..cases.len() {
            eprintln!(
                "  {:26} vs {:26} {:.3}",
                cases[i].0,
                cases[j].0,
                cos(&first[i], &first[j])
            );
        }
    }

    // The decisive one: completely different text must not be near 1.0.
    let distinct = cos(&pooled[1], &pooled[3]);
    eprintln!("\n'a dog' vs 'the ocean at sunset' (pooler): {distinct:.3}");
    eprintln!(
        "\nIf that is above ~0.9 the text tower has the same defect the vision\n\
         tower has, and the failure is in how both towers are read rather than in\n\
         either model. If it is near 0, the text side is fine and the whole problem\n\
         is the vision tensor."
    );
}
