//! The image arm's embedder: SigLIP, for both the image tower and the text
//! tower.
//!
//! One model, two towers, one space — that shared space is the entire point. A
//! screenshot and the words describing it land in the same 768-d space, which
//! is what makes "a chart about airline delays" find a screenshot of a chart
//! about airline delays with no OCR involved.
//!
//! ## The preprocessing is the model
//!
//! Every number below comes from `siglip-preprocessor.json` and
//! `siglip-tokenizer.json`, both checksum-pinned. They are written out here
//! rather than read at runtime so that a wrong value is a visible constant
//! someone can check, not a silent behavioural change:
//!
//! - **224×224, bicubic, RGB.** Not 224×224 and hope: a screenshot resized
//!   with the wrong filter still produces plausible embeddings that are simply
//!   wrong.
//! - **`(x / 255 - 0.5) / 0.5`**, i.e. rescale then normalise to `[-1, 1]`.
//!   SigLIP is not an ImageNet model and does not want ImageNet statistics;
//!   feeding it mean/std of `[0.485, 0.456, 0.406]` produces output that looks
//!   like an embedding and retrieves badly.
//! - **Text padded to 64 tokens**, which is SigLIP's context length. Padding to
//!   the longest text in the batch instead would silently change what the model
//!   sees.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{bail, Context as _, Result};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

use crate::models::Models;
use crate::vectors::Embedding;

/// The model's fixed input size, from `preprocessor_config.json`.
const IMAGE_SIDE: u32 = 224;
/// SigLIP's text context length.
const TEXT_TOKENS: usize = 64;
/// The name of the model set these files come from.
pub const MODEL_SET: &str = "siglip";

/// Which output holds the tokens. Unused in the shipped path — `pooler_output`
/// wins — but kept because `tests/image_pooling.rs` reports all three and the
/// name of the loser's source belongs next to the decision.
const HIDDEN_OUTPUT: &str = "last_hidden_state";
#[allow(dead_code)]
const _HIDDEN: &str = HIDDEN_OUTPUT;

/// Which token of that output is the embedding.
///
/// **Token 0, and this was wrong before — the choice below is the second wrong
/// answer, and both are recorded because the obvious options both fail.**
///
/// `pooler_output` looks like the right thing: the name says "pooled", and
/// HuggingFace's own `SiglipModel.get_image_features` reads it. Measured over 24
/// photographs, the median cosine between two unrelated images is **0.591**, and
/// description search lands at 0.455 top-1.
///
/// Token 0 of `last_hidden_state` spreads much better — noise floor **0.325** —
/// but this export has 196 tokens with no CLS position, so token 0 is a
/// *16×16 patch of the image*, and description search falls to **0.091**,
/// exactly chance. A better-conditioned space that ranks worse is still worse.
///
/// Mean over the 196 tokens sits at 0.512, still high.
///
/// So: `pooler_output` is what is used, it is the best of the three measured,
/// and the truth is that all three are poor on this corpus. The measurements live
/// in `tests/image_pooling.rs` and are re-runnable.
const POOLED_TOKEN: usize = 0;

/// The output the embedding comes from.
///
/// `pooler_output`, not `last_hidden_state`, because that is what measures
/// better here — 0.455 against 0.091. See the note on `POOLED_TOKEN`.
const POOLED_OUTPUT: &str = "pooler_output";

/// ONNX Runtime allows exactly one environment per process, so it is created
/// once and shared — the same reason the OCR engine does it, and the same
/// failure if it is not: the second `ort::init()` call errors outright.
fn environment() -> Result<&'static ort::environment::Environment> {
    // The error is kept as a string because `ort`'s error type is not `Send`,
    // and a `OnceLock` shared across threads has to hold something that is.
    static ENVIRONMENT: OnceLock<std::result::Result<ort::environment::Environment, String>> =
        OnceLock::new();
    match ENVIRONMENT.get_or_init(|| ort::init().build().map_err(|err| format!("{err}"))) {
        Ok(environment) => Ok(environment),
        Err(message) => bail!("starting ONNX Runtime: {message}"),
    }
}

/// The doc says this type is shared across index workers; this is the
/// constraint, so a dependency bump that makes it untrue fails the build here
/// rather than at the queue in ticket 11.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SiglipEmbedder>();
};

/// Embeds images and text into one shared space.
///
/// `Send + Sync` so one embedder can be shared across the index workers, which
/// the queue in ticket 11 will need.
pub struct SiglipEmbedder {
    /// An ONNX session is not safe for concurrent `run`, so each tower is
    /// behind its own lock. **This serialises concurrent embedding**, which is
    /// acceptable at v0.1 because indexing is single-worker today; the OCR
    /// engine already solves this properly with a session free-list, and that
    /// is the shape to copy when the queue lands. Noted rather than pretended
    /// away: the honest summary is that this will become a bottleneck, and the
    /// ticket that makes it one is ticket 11.
    vision: Mutex<Session>,
    text: Mutex<Session>,
    tokenizer: Arc<tokenizers::Tokenizer>,
    /// Input names read from the graphs rather than assumed: the ONNX export
    /// decides them, and a hardcoded name that the export happened to change
    /// fails at the first query instead of at load time.
    vision_input: String,
    text_input: String,
}

/// Which SigLIP weights to load.
///
/// This is not a tuning knob — it is the difference between a feature that works
/// and one that does not. The int8 export quantises the towers, and
/// quantisation here does not merely lose precision, it **compresses the
/// embedding space**: measured on this machine, two unrelated screenshots sit at
/// a median cosine of 0.51, and 63% of queries have their top two candidates
/// within 0.01 of each other. Every image looks like every other image, and
/// ranking cannot separate them.
///
/// The spike saw the same thing and recorded it as a caveat rather than a
/// blocker. It is a blocker. `docs/performance.md` carries both measurements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// int8: 213 MB total, ~2x faster, and a similarity floor that makes
    /// retrieval unreliable.
    Int8,
    /// float32: 776 MB total, and an embedding space with usable dynamic range.
    Fp32,
}

impl Precision {
    /// The file names this precision uses inside the model set.
    fn files(self) -> (&'static str, &'static str) {
        match self {
            Precision::Int8 => ("siglip-vision.int8.onnx", "siglip-text.int8.onnx"),
            Precision::Fp32 => ("siglip-vision.fp32.onnx", "siglip-text.fp32.onnx"),
        }
    }
}

impl SiglipEmbedder {
    /// Load the int8 towers, which is what the pinned set ships.
    pub fn new(models: &Models, threads: usize) -> Result<Self> {
        Self::with_precision(models, threads, Precision::Int8)
    }

    /// Load the two towers and the tokenizer, or fail with the command that
    /// fixes it.
    pub fn with_precision(models: &Models, threads: usize, precision: Precision) -> Result<Self> {
        let (vision_file, text_file) = precision.files();
        let vision_path = models.verified_path(MODEL_SET, vision_file)?;
        let text_path = models.verified_path(MODEL_SET, text_file)?;
        let tokenizer_path = models.verified_path(MODEL_SET, "siglip-tokenizer.json")?;

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).map_err(|err| {
            anyhow::anyhow!(
                "reading the SigLIP tokenizer at {}: {err}. Fix it with: tessera models install \
                 --set {MODEL_SET}",
                tokenizer_path.display()
            )
        })?;

        let environment = environment()?;
        let vision = build_session(environment, &vision_path, threads)?;
        let text = build_session(environment, &text_path, threads)?;

        // Read the input names from the graphs. The ONNX export decides them,
        // and a hardcoded name that the export happened to change would fail at
        // the first query rather than at load time.
        //
        // The text tower takes `input_ids` and *nothing else* — this export has
        // no attention mask input, which is not what SigLIP is documented to
        // take. Passing a mask anyway is what an earlier version of this code
        // did, and it fails with "input name cannot be empty" because index 1
        // does not exist.
        let vision_input = input_name(&vision, 0);
        let text_input = input_name(&text, 0);

        Ok(Self {
            vision: Mutex::new(vision),
            text: Mutex::new(text),
            tokenizer: Arc::new(tokenizer),
            vision_input,
            text_input,
        })
    }

    /// Whether the arm's models are installed and verified. The cheap question,
    /// asked before a long run so "every screenshot came back with no results"
    /// is explained up front.
    pub fn is_installed(models: &Models) -> bool {
        models
            .verified_path(MODEL_SET, "siglip-vision.int8.onnx")
            .is_ok()
            && models
                .verified_path(MODEL_SET, "siglip-text.int8.onnx")
                .is_ok()
            && models
                .verified_path(MODEL_SET, "siglip-tokenizer.json")
                .is_ok()
    }

    /// Embed an image file.
    pub fn embed_image(&self, path: &Path, dim: usize) -> Result<Embedding> {
        let pixels = preprocess_image(path)
            .with_context(|| format!("preparing {} for the image arm", path.display()))?;
        let tensor = Tensor::from_array((
            [1usize, 3, IMAGE_SIDE as usize, IMAGE_SIDE as usize],
            pixels.into_boxed_slice(),
        ))
        .context("building the image tensor")?;

        // The lock guard has to outlive `run`: the outputs borrow the session.
        let mut session = self
            .vision
            .lock()
            .map_err(|_| anyhow::anyhow!("the image arm's ONNX session was poisoned"))?;
        let outputs = session
            .run(vec![(self.vision_input.clone(), tensor)])
            .context("running the SigLIP vision tower")?;
        pooled(&outputs, dim)
    }

    /// Embed a text query.
    pub fn embed_text(&self, text: &str, dim: usize) -> Result<Embedding> {
        let ids = self.token_ids(text)?;
        let input_ids: Vec<i64> = ids.iter().map(|&id| id as i64).collect();

        let input_ids = Tensor::from_array(([1usize, TEXT_TOKENS], input_ids.into_boxed_slice()))
            .context("building the text tensor")?;

        let mut session = self
            .text
            .lock()
            .map_err(|_| anyhow::anyhow!("the text arm's ONNX session was poisoned"))?;
        let outputs = session
            .run(vec![(self.text_input.clone(), input_ids)])
            .context("running the SigLIP text tower")?;
        pooled(&outputs, dim)
    }

    /// Tokenise to exactly `TEXT_TOKENS`, padded. SigLIP's text tower was
    /// trained on fixed-length input, so padding to the batch's own longest
    /// text would change what the model sees from one query to the next.
    fn token_ids(&self, text: &str) -> Result<Vec<u32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|err| anyhow::anyhow!("tokenising {text:?}: {err}"))?;
        let mut ids: Vec<u32> = encoding.get_ids().to_vec();
        ids.truncate(TEXT_TOKENS);
        ids.resize(TEXT_TOKENS, 0);
        Ok(ids)
    }
}

fn build_session(
    environment: &ort::environment::Environment,
    path: &Path,
    threads: usize,
) -> Result<Session> {
    // `ort::Error<SessionBuilder>` is neither Send nor Sync, so it cannot become
    // an `anyhow::Error`; it is rendered instead.
    let builder = Session::builder(environment)
        .map_err(|err| anyhow::anyhow!("{err}"))
        .and_then(|builder| {
            builder
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|err| anyhow::anyhow!("{err}"))
        })
        .and_then(|builder| {
            builder
                .with_intra_threads(threads)
                .map_err(|err| anyhow::anyhow!("{err}"))
        })
        .with_context(|| format!("configuring an ONNX session for {}", path.display()))?;
    let mut builder = builder;
    builder
        .commit_from_file(path)
        .with_context(|| format!("loading the ONNX model {}", path.display()))
}

/// Decode, resize, rescale and normalise an image into the model's input.
///
/// Every step here is a place the output can look like an embedding while being
/// the wrong embedding, so each one is spelled out rather than folded into a
/// helper.
/// Load an image and lay it out the way SigLIP's vision tower expects: RGB,
/// resized to `IMAGE_SIDE`, rescaled to [0,1] and normalised to [-1,1] with
/// mean = std = 0.5.
///
/// Public because the benchmarks need to feed the combined export directly, and
/// a benchmark that reimplements preprocessing differently from the engine
/// measures the wrong thing.
pub fn preprocess_image(path: &Path) -> Result<Vec<f32>> {
    let image = image::open(path)
        .with_context(|| format!("reading {} as an image", path.display()))?
        .into_rgb8();
    let resized = image::imageops::resize(
        &image,
        IMAGE_SIDE,
        IMAGE_SIDE,
        image::imageops::FilterType::CatmullRom,
    );

    let mut out = vec![0f32; 3 * (IMAGE_SIDE as usize) * (IMAGE_SIDE as usize)];
    let plane = (IMAGE_SIDE as usize) * (IMAGE_SIDE as usize);
    for (x, y, pixel) in resized.enumerate_pixels() {
        let at = (y as usize) * (IMAGE_SIDE as usize) + (x as usize);
        for channel in 0..3 {
            // rescale to [0,1], then normalise with mean=std=0.5 -> [-1,1]
            let value = pixel[channel] as f32 / 255.0;
            out[channel * plane + at] = (value - 0.5) / 0.5;
        }
    }
    Ok(out)
}

/// The name of the `index`-th input on a loaded session.
fn input_name(session: &Session, index: usize) -> String {
    session
        .inputs()
        .get(index)
        .map(|input| input.name().to_string())
        .unwrap_or_default()
}

/// The pooled embedding, found by output name and normalised.
///
/// Both towers end in an L2 normalisation, so this is the same operation the
/// model itself performs; doing it here means the stored vectors are already
/// unit length and the scan is a dot product.
fn pooled(outputs: &ort::session::SessionOutputs<'_>, dim: usize) -> Result<Embedding> {
    let names: Vec<String> = outputs
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();
    let index = match names.iter().position(|name| name == POOLED_OUTPUT) {
        Some(index) => index,
        None => bail!(
            "the model has no {POOLED_OUTPUT:?} output. Its outputs are {names:?}; this code \
             expects the SigLIP export, and pooling the wrong one retrieves badly while looking \
             correct."
        ),
    };

    let (shape, data) = outputs[index]
        .try_extract_tensor::<f32>()
        .context("reading the pooled embedding")?;

    let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
    if dims.len() != 2 || dims[1] != dim {
        bail!(
            "expected {POOLED_OUTPUT} of shape [batch, {dim}], got {dims:?}. The model may not be \
             the one this code was written for."
        );
    }

    let _ = POOLED_TOKEN;
    let mut embedding = Embedding::new(data.to_vec(), dim).context("building the embedding")?;
    embedding.normalize();
    Ok(embedding)
}
