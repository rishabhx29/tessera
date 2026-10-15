//! Image captioning: what a picture says, as text.
//!
//! ## Why this exists as a separate thing from the embedding arm
//!
//! "Search by description" means finding an image by what is *in* it, with no
//! text appearing anywhere in the picture. The SigLIP arm tries to answer that by
//! placing words and pixels in one shared 768-d space, and measurement puts it at
//! 0.455 top-1 on photographs.
//!
//! An earlier version of this note added that the arm "barely responds to the
//! input at all", citing 0.897 between a solid black and a solid white image.
//! That number is real and it is not evidence of a fault: it is **anisotropy**,
//! two images sharing a direction because they are the same modality, which is
//! what every healthy transformer embedding does. The cross-modal measurement it
//! was being read as sits near zero (-0.033 for a deliberately unrelated pair),
//! so the towers do respond to their input and to each other. See the correction
//! in `docs/performance.md`.
//!
//! Captioning answers the same question differently: the model looks at the image
//! and writes a sentence about it, and that sentence is indexed as **text**. The
//! retrieval is then done by the arm already measured at **0.965** on this
//! machine's real corpus.
//!
//! Two consequences worth stating plainly, because they are the reason to prefer
//! this route:
//!
//! - **The image becomes findable by words never seen in training.** A caption
//!   containing the word "ocean" is retrievable by any query mentioning the
//!   ocean, whatever else the query says. An embedding only retrieves what sits
//!   near it geometrically.
//! - **A match can be explained.** The result can show the caption, so the user
//!   sees *why* the image came back. No embedding can do that.
//!
//! The cost is that this is autoregressive generation: one forward pass per
//! caption, on the indexing path only.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context as _, Result};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

/// The model set's name, matching `lock.json`.
pub const MODEL_SET: &str = "caption";

/// ViT-GPT2's fixed input size.
const SIDE: u32 = 224;
/// Its tokeniser's context length.
const TOKENS: usize = 1024;
/// The first id in the vocabulary, which GPT-2 uses as the start of a caption.
const BOS: i64 = 50256;
/// The last id, which GPT-2 uses as end-of-text.
const EOS: i64 = 50256;

/// Describes images in words.
pub struct Captioner {
    encoder: Mutex<Session>,
    decoder: Mutex<Session>,
    tokenizer: std::sync::Arc<tokenizers::Tokenizer>,
    decoder_input: String,
}

impl Captioner {
    /// Load the encoder, the merged decoder and the tokeniser.
    pub fn new(models: &crate::models::Models, threads: usize) -> Result<Self> {
        let encoder_path = models.verified_path(MODEL_SET, "encoder.onnx")?;
        let decoder_path = models.verified_path(MODEL_SET, "decoder.onnx")?;
        let tokenizer_path = models.verified_path(MODEL_SET, "tokenizer.json")?;

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).map_err(|err| {
            anyhow::anyhow!(
                "reading the captioning tokeniser at {}: {err}",
                tokenizer_path.display()
            )
        })?;

        let environment = caption_environment()?;
        let encoder = build_session(environment, &encoder_path, threads)?;
        let decoder = build_session(environment, &decoder_path, threads)?;
        let decoder_input = decoder
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .ok_or_else(|| anyhow::anyhow!("the decoder has no inputs"))?;

        Ok(Self {
            encoder: Mutex::new(encoder),
            decoder: Mutex::new(decoder),
            tokenizer: std::sync::Arc::new(tokenizer),
            decoder_input,
        })
    }

    /// Whether the captioning models are installed. The cheap question.
    pub fn is_installed(models: &crate::models::Models) -> bool {
        models.verified_path(MODEL_SET, "encoder.onnx").is_ok()
            && models.verified_path(MODEL_SET, "decoder.onnx").is_ok()
            && models.verified_path(MODEL_SET, "tokenizer.json").is_ok()
    }

    /// A one-sentence description of the image.
    ///
    /// Deliberately a single caption, not a list of candidates: a search index
    /// wants the most likely reading of a picture, and five alternatives would
    /// put five spellings of the same scene into the text arm's way.
    pub fn caption(&self, path: &Path) -> Result<String> {
        let pixels = preprocess(path)?;
        let tensor = Tensor::from_array((
            [1usize, 3, SIDE as usize, SIDE as usize],
            pixels.into_boxed_slice(),
        ))
        .context("building the image tensor")?;

        // The encoder turns the image into the hidden states the decoder
        // conditions on. Its own guard must outlive the run, because the
        // outputs borrow the session.
        let mut encoder = self
            .encoder
            .lock()
            .map_err(|_| anyhow::anyhow!("the captioning encoder was poisoned"))?;
        let encoder_name = encoder_input_name(&encoder)?;
        let encoded = encoder
            .run(vec![(encoder_name, tensor)])
            .context("encoding the image")?;

        // The merged decoder takes the image features *and* the tokens so far.
        // The features have to outlive the decoder's outputs too.
        let features: Vec<f32> = encoded[0]
            .try_extract_tensor::<f32>()
            .context("reading the encoder features")?
            .1
            .to_vec();
        let _ = features;

        let mut decoder = self
            .decoder
            .lock()
            .map_err(|_| anyhow::anyhow!("the captioning decoder was poisoned"))?;

        // One token at a time: start from BOS, ask for the next, append, stop at
        // EOS or the context length.
        let mut tokens: Vec<i64> = vec![BOS];
        for _ in 0..32 {
            let input_ids =
                Tensor::from_array(([1usize, tokens.len()], tokens.clone().into_boxed_slice()))
                    .context("building the token tensor")?;
            let mask: Vec<i64> = tokens.iter().map(|_| 1).collect();
            let attention = Tensor::from_array(([1usize, tokens.len()], mask.into_boxed_slice()))
                .context("building the attention mask")?;

            let outputs = decoder
                .run(vec![
                    (self.decoder_input.clone(), input_ids),
                    ("attention_mask".to_string(), attention),
                ])
                .context("running the captioning decoder")?;

            let (shape, logits) = outputs[0]
                .try_extract_tensor::<f32>()
                .context("reading the decoder output")?;
            let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
            // The last position's logits are the distribution over the next
            // token; the vocabulary width comes from the same tensor.
            let vocab = *dims.last().unwrap_or(&TOKENS);
            let last = dims[dims.len() - 2].saturating_sub(1) * vocab;
            let next = argmax(&logits[last..last + vocab]);
            if next == EOS {
                break;
            }
            tokens.push(next);
        }
        drop(decoder);

        decode(&self.tokenizer, &tokens)
    }
}

fn caption_environment() -> Result<&'static ort::environment::Environment> {
    use std::sync::OnceLock;
    static ENV: OnceLock<std::result::Result<ort::environment::Environment, String>> =
        OnceLock::new();
    match ENV.get_or_init(|| ort::init().build().map_err(|err| format!("{err}"))) {
        Ok(env) => Ok(env),
        Err(message) => anyhow::bail!("starting ONNX Runtime: {message}"),
    }
}

fn build_session(
    environment: &ort::environment::Environment,
    path: &Path,
    threads: usize,
) -> Result<Session> {
    let mut builder = Session::builder(environment)
        .map_err(|err| anyhow::anyhow!("{err}"))
        .and_then(|builder| {
            builder
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|err| anyhow::anyhow!("{err}"))
                .and_then(|b| {
                    b.with_intra_threads(threads)
                        .map_err(|e| anyhow::anyhow!("{e}"))
                })
        })
        .with_context(|| format!("configuring a session for {}", path.display()))?;
    builder
        .commit_from_file(path)
        .with_context(|| format!("loading {}", path.display()))
}

fn encoder_input_name(session: &Session) -> Result<String> {
    session
        .inputs()
        .first()
        .map(|input| input.name().to_string())
        .ok_or_else(|| anyhow::anyhow!("the encoder has no inputs"))
}

fn argmax(values: &[f32]) -> i64 {
    values
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(i, _)| i as i64)
        .unwrap_or(EOS)
}

fn decode(tokenizer: &tokenizers::Tokenizer, tokens: &[i64]) -> Result<String> {
    let ids: Vec<u32> = tokens
        .iter()
        .skip(1)
        .filter(|t| **t >= 0)
        .map(|t| *t as u32)
        .collect();
    tokenizer
        .decode(&ids, false)
        .map_err(|err| anyhow::anyhow!("decoding the caption: {err}"))
}

/// The encoder's input: an RGB image at its fixed size, rescaled to `[0, 1]`.
///
/// Note this is the **ViT-GPT2** preprocessing, which is not the SigLIP
/// normalisation. This model was trained on plain `[0, 1]` pixels, so reusing the
/// `(x - 0.5) / 0.5` step from the embedding arm would produce garbage captions
/// that look like output. The two models genuinely want different input.
fn preprocess(path: &Path) -> Result<Vec<f32>> {
    let image = image::open(path)
        .with_context(|| format!("reading {} as an image", path.display()))?
        .into_rgb8();
    let resized =
        image::imageops::resize(&image, SIDE, SIDE, image::imageops::FilterType::CatmullRom);
    let plane = (SIDE as usize) * (SIDE as usize);
    let mut out = vec![0f32; 3 * plane];
    for (x, y, pixel) in resized.enumerate_pixels() {
        let at = (y as usize) * (SIDE as usize) + (x as usize);
        for channel in 0..3 {
            out[channel * plane + at] = pixel[channel] as f32 / 255.0;
        }
    }
    Ok(out)
}
