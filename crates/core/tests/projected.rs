//! Does the combined SigLIP export actually separate things?
//!
//! `optimisation_level.rs` recorded the defect: the bare towers put solid black
//! 0.897 from solid white and 'a dog' 0.808 from 'the ocean at sunset'. Those
//! are two representations that were never trained to be compared, because
//! SigLIP puts each tower's output through its own projection head first, and
//! Xenova ships those heads only inside the combined `model.onnx`.
//!
//! The combined export takes both `input_ids` and `pixel_values` in one pass -
//! it has to, because it also emits the two similarity logit tensors - and
//! returns `text_embeds` and `image_embeds` from that single pass. So one run
//! per (image, text) pair yields both sides, which is exactly what the
//! cross-modal question needs.
//!
//! ```
//! cargo test --release -p tessera-core --test projected -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::OnceLock;

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

const TOKENS: usize = 64;
const SIDE: usize = 224;

fn environment() -> &'static ort::environment::Environment {
    static ENV: OnceLock<Result<ort::environment::Environment, String>> = OnceLock::new();
    match ENV.get_or_init(|| ort::init().build().map_err(|e| format!("{e}"))) {
        Ok(e) => e,
        Err(m) => panic!("onnx runtime: {m}"),
    }
}

fn models_dir() -> PathBuf {
    std::env::var("TESSERA_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            tessera_core::cli::models_dir(
                &tessera_core::cli::default_data_dir().expect("a data dir"),
                None,
            )
        })
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

/// One pass through both towers: the projected embedding of `text` and of the
/// flat field `value`. 0.0 is black, 1.0 is white.
fn embed_pair(
    session: &mut Session,
    tokenizer: &tokenizers::Tokenizer,
    text_index: usize,
    image_index: usize,
    text: &str,
    value: f32,
) -> (Vec<f32>, Vec<f32>) {
    let encoding = tokenizer.encode(text, true).expect("encode");
    let mut ids: Vec<i64> = encoding.get_ids().iter().map(|t| *t as i64).collect();
    ids.truncate(TOKENS);
    ids.resize(TOKENS, 0);

    let ids = Tensor::from_array(([1usize, TOKENS], ids.into_boxed_slice())).expect("ids");
    let pixels: Vec<f32> = vec![value; SIDE * SIDE * 3];
    let pixels =
        Tensor::from_array(([1usize, 3, SIDE, SIDE], pixels.into_boxed_slice())).expect("pixels");

    // The combined graph needs both inputs and they have different element
    // types, so a plain `Vec` of one dtype will not do; `inputs!` is the macro
    // that pairs each name with its own type.
    let out = session
        .run(ort::inputs![
            "input_ids" => ids,
            "pixel_values" => pixels,
        ])
        .expect("run both towers");

    let (_, t) = out[text_index]
        .try_extract_tensor::<f32>()
        .expect("text_embeds");
    let (_, i) = out[image_index]
        .try_extract_tensor::<f32>()
        .expect("image_embeds");
    let mut t = t.to_vec();
    let mut i = i.to_vec();
    unit(&mut t);
    unit(&mut i);
    (t, i)
}

#[test]
#[ignore = "needs the combined siglip export"]
fn the_combined_export_projects_text_and_image_into_one_space() {
    let dir = models_dir();
    let path = dir.join("siglip-full.uint8.onnx");
    assert!(
        path.is_file(),
        "the combined export is not installed at {}",
        path.display()
    );

    let builder = Session::builder(environment()).expect("builder");
    let mut configured = builder
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .expect("optimise");
    let mut session = configured.commit_from_file(&path).expect("load");

    let names: Vec<String> = session
        .outputs()
        .iter()
        .map(|o| o.name().to_string())
        .collect();
    eprintln!("outputs: {names:?}");
    let index_of = |wanted: &str| {
        names
            .iter()
            .position(|n| n == wanted)
            .unwrap_or_else(|| panic!("{wanted} is not an output; got {names:?}"))
    };
    let (ti, ii) = (index_of("text_embeds"), index_of("image_embeds"));

    let tokenizer =
        tokenizers::Tokenizer::from_file(dir.join("siglip-tokenizer.json")).expect("tokeniser");

    // Two runs give all four vectors: each yields one text side and one image
    // side, so the pairs are chosen to span text-vs-text and image-vs-image.
    let (dog, black) = embed_pair(&mut session, &tokenizer, ti, ii, "a dog", 0.0);
    let (ocean, white) = embed_pair(&mut session, &tokenizer, ti, ii, "the ocean at sunset", 1.0);

    eprintln!("\ndim {}", dog.len());
    let tt = cos(&dog, &ocean);
    let ii_sep = cos(&black, &white);
    let same = cos(&dog, &black);
    let crossed = cos(&ocean, &white);
    eprintln!("text   'a dog' vs 'the ocean at sunset' = {tt:.3}");
    eprintln!("image  black vs white                    = {ii_sep:.3}");
    eprintln!("cross  'a dog' vs black                  = {same:.3}");
    eprintln!("cross  'the ocean at sunset' vs white    = {crossed:.3}");

    // Absolute cosine cannot judge an embedding space. Raw transformer
    // embeddings are anisotropic - they share a large common direction, so two
    // unrelated sentences still sit at 0.82 and solid black at 0.91 from solid
    // white - and that is true of a healthy model too. Asserting on those
    // numbers measures anisotropy, not quality, which is what an earlier
    // version of this test did and got wrong.
    //
    // What does carry meaning is cross-modal alignment: once both towers have
    // been projected, an image and a sentence that do not describe each other
    // must sit near zero. Comparing unprojected towers pinned these at 0.9 with
    // every pair, which is the defect this test was written to catch.
    for (label, value) in [
        ("'a dog' vs black", same),
        ("'the ocean at sunset' vs white", crossed),
    ] {
        assert!(
            value.abs() < 0.3,
            "{label} is {value:.3}; unrelated pairs should sit near zero once projected, \
             so the two towers are still not in one space"
        );
    }
}
