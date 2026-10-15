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
/// How many tokens a caption may run to. Real captions are a dozen or so; the
/// cap exists so a decoder that never emits end-of-text cannot spin.
const MAX_CAPTION_TOKENS: usize = 32;

/// Describes images in words.
pub struct Captioner {
    encoder: Mutex<Session>,
    decoder: Mutex<Session>,
    tokenizer: std::sync::Arc<tokenizers::Tokenizer>,
    feed: DecoderFeed,
}

/// The decoder's named inputs, read once when it loads.
///
/// The merged ViT-GPT2 export is one fixed graph rather than a generator with
/// defaults. Every step it wants the token ids, the image features, the whole
/// set of cache tensors — empty, on the first pass — and a boolean choosing
/// between its two branches. It has no `attention_mask` input at all, which is
/// what this code used to send it, so the run failed outright and the arm never
/// produced a caption.
struct DecoderFeed {
    input_ids: String,
    encoder_hidden_states: String,
    /// Every `past_key_values.N.key` and `.value`, named explicitly: ONNX
    /// Runtime checks a static dimension's shape even when the tensor is empty.
    past: Vec<String>,
    /// The shape of a cache with nothing in it: `[batch, heads, 0, head_dim]`.
    /// Taken from the graph's own declaration rather than written down here, so
    /// the attention width cannot drift out of step with the model.
    empty_cache: Vec<usize>,
    use_cache_branch: String,
}

impl DecoderFeed {
    /// Read the decoder's signature. Anything that is not the merged export this
    /// code was written against fails here, by name, rather than later as a
    /// caption that came out empty for no stated reason.
    fn discover(decoder: &Session) -> Result<Self> {
        let mut input_ids = None;
        let mut encoder_hidden_states = None;
        let mut use_cache_branch = None;
        let mut past = Vec::new();
        let mut empty_cache = None;

        for input in decoder.inputs() {
            let name = input.name();
            match name {
                "input_ids" => input_ids = Some(name.to_string()),
                "encoder_hidden_states" => encoder_hidden_states = Some(name.to_string()),
                "use_cache_branch" => use_cache_branch = Some(name.to_string()),
                _ if name.starts_with("past_key_values.") => {
                    if empty_cache.is_none() {
                        if let ort::value::ValueType::Tensor { shape, .. } = input.dtype() {
                            // A dynamic dimension is declared as -1, which here
                            // means the same as 0: nothing cached yet.
                            let mut dims: Vec<usize> = shape
                                .iter()
                                .map(|d| usize::try_from(*d).unwrap_or(0))
                                .collect();
                            if dims.len() == 4 {
                                dims[0] = 1;
                                dims[2] = 0;
                                empty_cache = Some(dims);
                            }
                        }
                    }
                    past.push(name.to_string());
                }
                _ => {}
            }
        }

        let (
            Some(input_ids),
            Some(encoder_hidden_states),
            Some(use_cache_branch),
            Some(empty_cache),
        ) = (
            input_ids,
            encoder_hidden_states,
            use_cache_branch,
            empty_cache,
        )
        else {
            anyhow::bail!(
                "the captioning decoder is not the merged ViT-GPT2 export this code expects: it \
                 declares no input_ids, encoder_hidden_states, use_cache_branch or \
                 past_key_values.* (it declares {})",
                decoder
                    .inputs()
                    .iter()
                    .map(|input| input.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        };

        Ok(Self {
            input_ids,
            encoder_hidden_states,
            past,
            empty_cache,
            use_cache_branch,
        })
    }
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
        let feed = DecoderFeed::discover(&decoder)?;

        Ok(Self {
            encoder: Mutex::new(encoder),
            decoder: Mutex::new(decoder),
            tokenizer: std::sync::Arc::new(tokenizer),
            feed,
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

        // Copied out rather than borrowed: the features are handed to the
        // decoder once per generated token, and a tensor can only be moved into
        // a run once.
        let (feature_shape, feature_data) = encoded[0]
            .try_extract_tensor::<f32>()
            .context("reading the encoder features")?;
        let feature_dims: Vec<usize> = feature_shape.iter().map(|d| *d as usize).collect();
        let features: Vec<f32> = feature_data.to_vec();
        drop(encoded);

        let mut decoder = self
            .decoder
            .lock()
            .map_err(|_| anyhow::anyhow!("the captioning decoder was poisoned"))?;

        // One token at a time. The cache is declared empty on every step and the
        // whole sequence is re-sent, which costs a repeat of a sequence at most a
        // few dozen tokens long and takes a stale-cache bug out of the loop
        // entirely.
        let mut tokens: Vec<i64> = vec![BOS];
        for _ in 0..MAX_CAPTION_TOKENS {
            let features =
                Tensor::from_array((feature_dims.clone(), features.clone().into_boxed_slice()))
                    .context("rebuilding the image features")?;
            let input_ids =
                Tensor::from_array(([1usize, tokens.len()], tokens.clone().into_boxed_slice()))
                    .context("building the token tensor")?;
            let branch =
                Tensor::from_array(([1usize], vec![false])).context("building the cache switch")?;

            let mut request: Vec<(String, ort::value::DynValue)> =
                Vec::with_capacity(self.feed.past.len() + 3);
            request.push((self.feed.input_ids.clone(), input_ids.into_dyn()));
            request.push((self.feed.encoder_hidden_states.clone(), features.into_dyn()));
            request.push((self.feed.use_cache_branch.clone(), branch.into_dyn()));
            for name in &self.feed.past {
                let empty = Tensor::from_array((
                    self.feed.empty_cache.clone(),
                    Vec::<f32>::new().into_boxed_slice(),
                ))
                .context("building an empty cache entry")?;
                request.push((name.clone(), empty.into_dyn()));
            }

            let outputs = decoder
                .run(request)
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

/// The encoder's input: an RGB image at its fixed size, prepared the way the
/// model's own pinned preprocessor config says.
///
/// The pinned `preprocessor.json` is the authority here, and it is not the
/// SigLIP one: `do_rescale` with a `rescale_factor` of 1/255, then
/// `do_normalize` with a mean and standard deviation of 0.5, resized with
/// `resample: 2`, which is bilinear. Applied, that is the same `(x - 0.5) / 0.5`
/// shape the embedding arm uses — a coincidence of both models being trained
/// around mean 0.5, not a shared convention. Each arm takes its numbers from its
/// own pinned config.
fn preprocess(path: &Path) -> Result<Vec<f32>> {
    const MEAN: f32 = 0.5;
    const STD: f32 = 0.5;
    let image = image::open(path)
        .with_context(|| format!("reading {} as an image", path.display()))?
        .into_rgb8();
    let resized =
        image::imageops::resize(&image, SIDE, SIDE, image::imageops::FilterType::Triangle);
    let plane = (SIDE as usize) * (SIDE as usize);
    let mut out = vec![0f32; 3 * plane];
    for (x, y, pixel) in resized.enumerate_pixels() {
        let at = (y as usize) * (SIDE as usize) + (x as usize);
        for channel in 0..3 {
            out[channel * plane + at] = (pixel[channel] as f32 / 255.0 - MEAN) / STD;
        }
    }
    Ok(out)
}
