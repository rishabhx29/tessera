//! Is the near-constant output our harness's fault, or the model's?
//!
//! Two towers from the same repository degrade in the same suspicious way: the
//! vision tower's `pooler_output` scores solid black 0.897 against solid white,
//! and the text tower's scores 'a dog' 0.808 against 'the ocean at sunset'. Two
//! unrelated models failing identically points at something shared, and the
//! candidates are the ones this test does not check elsewhere:
//!
//! - the graph optimisation level the session was built with (Level3 here), and
//! - the output *names* versus the output *indices* the reader assumes
//!
//! Both are free to check and neither needs a download, which is why this runs
//! before trying a different export.
//!
//! ```
//! cargo test --release -p tessera-core --test optimisation_level -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::OnceLock;

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

const DIM: usize = 768;
const TOKENS: usize = 64;

/// The levels worth trying, cheapest first. Level3 is what every other test in
/// this repository uses, so it is the suspect.
const LEVELS: [(GraphOptimizationLevel, &str); 3] = [
    (
        GraphOptimizationLevel::Level3,
        "Level3 (what the engine uses)",
    ),
    (GraphOptimizationLevel::Level1, "Level1"),
    (GraphOptimizationLevel::Disable, "Disable"),
];

fn environment() -> &'static ort::environment::Environment {
    static ENV: OnceLock<Result<ort::environment::Environment, String>> = OnceLock::new();
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

fn cos(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn unit(v: &mut [f32]) {
    let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > f32::EPSILON {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

/// The pooled output, by name. If the name is absent the reader's index
/// assumption is the bug, and this says so rather than silently reading a
/// different tensor.
fn pooled_by_name(session: &mut Session, ids: &[i64]) -> Option<Vec<f32>> {
    let wanted = session
        .outputs()
        .iter()
        .map(|o| o.name().to_string())
        .position(|n| n == "pooler_output")?;
    let tensor = Tensor::from_array(([1usize, TOKENS], ids.to_vec().into_boxed_slice())).ok()?;
    let out = session.run(vec![("input_ids".to_string(), tensor)]).ok()?;
    let (_, pooled) = out[wanted].try_extract_tensor::<f32>().ok()?;
    let mut v = pooled.to_vec();
    unit(&mut v);
    Some(v)
}

#[test]
#[ignore = "needs the siglip model set"]
fn a_graph_the_runtime_does_not_reoptimise_still_sees_different_texts() {
    let models = models();
    let path = models
        .verified_path("siglip", "siglip-text.int8.onnx")
        .expect("int8 text tower");
    let tokenizer = tokenizers::Tokenizer::from_file(
        models
            .verified_path("siglip", "siglip-tokenizer.json")
            .expect("tokeniser"),
    )
    .expect("tokeniser");

    fn ids_for(tokenizer: &tokenizers::Tokenizer, text: &str) -> Vec<i64> {
        let encoding = tokenizer.encode(text, true).expect("encode");
        let mut ids: Vec<i64> = encoding.get_ids().iter().map(|t| *t as i64).collect();
        ids.truncate(TOKENS);
        ids.resize(TOKENS, 0);
        ids
    }

    let dog = ids_for(&tokenizer, "a dog");
    let ocean = ids_for(&tokenizer, "the ocean at sunset");
    // Prove the inputs really differ before blaming the graph for ignoring them.
    assert_ne!(dog, ocean, "the tokeniser produced identical ids");
    assert_eq!(DIM, 768);

    let mut separations: Vec<(&str, f32)> = Vec::new();

    for (level, label) in LEVELS {
        let builder = match Session::builder(environment()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{label:34} no builder: {e}");
                continue;
            }
        };
        let mut configured = match builder.with_optimization_level(level) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{label:34} could not be configured: {e}");
                continue;
            }
        };
        let session = match configured.commit_from_file(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{label:34} could not be committed: {e}");
                continue;
            }
        };
        let mut session = session;

        let names: Vec<String> = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        let (Some(a), Some(b)) = (
            pooled_by_name(&mut session, &dog),
            pooled_by_name(&mut session, &ocean),
        ) else {
            eprintln!("{label:34} outputs are {names:?}, no pooler_output among them");
            continue;
        };

        let separation = cos(&a, &b);
        eprintln!("{label:34} outputs {names:?}");
        eprintln!("{label:34} 'a dog' vs 'the ocean at sunset' = {separation:.3}");
        if label.starts_with("Disable") {
            // Raw values, so a plumbing fault is distinguishable from a dead
            // model. A broken extraction tends to give identical bytes; a model
            // that ignores its input gives *differing* bytes that all point the
            // same way. Those need opposite fixes, so the test must tell them
            // apart rather than report one cosine for both.
            let raw_a = pooled_by_name(&mut session, &dog).expect("raw a");
            let raw_b = pooled_by_name(&mut session, &ocean).expect("raw b");
            let norm_a: f32 = raw_a.iter().map(|x| x * x).sum::<f32>().sqrt();
            let norm_b: f32 = raw_b.iter().map(|x| x * x).sum::<f32>().sqrt();
            eprintln!("  norm: {norm_a:.4} vs {norm_b:.4}");
            let differing = raw_a
                .iter()
                .zip(&raw_b)
                .filter(|(x, y)| (**x - **y).abs() > 1e-6)
                .count();
            eprintln!(
                "  dimensions that actually differ: {differing} of {}",
                raw_a.len()
            );
            let top = raw_a
                .iter()
                .enumerate()
                .max_by(|x, y| x.1.abs().partial_cmp(&y.1.abs()).expect("no NaN"))
                .expect("not empty");
            eprintln!("  largest |component| at dim {} = {:.4}", top.0, top.1);
        }
        separations.push((label, separation));
    }

    // Two unrelated sentences should be nowhere near orthogonal. If a lower
    // optimisation level separates them, the near-constant output was this
    // harness's fusion pass, not the export - and the engine has been reading
    // a fused graph for its whole life.
    assert!(
        !separations.is_empty(),
        "no optimisation level could be tested"
    );
    for (label, separation) in &separations {
        eprintln!("  {label:34} {separation:.3}");
    }
    let (best_label, best) = separations
        .iter()
        .min_by(|a, b| a.1.partial_cmp(&b.1).expect("no NaN cosine"))
        .expect("at least one level");
    assert!(
        *best < 0.6,
        "unrelated sentences stay near-constant at every optimisation level \
         (best {best:.3} at {best_label}). The graph is not what makes it flat."
    );
}
