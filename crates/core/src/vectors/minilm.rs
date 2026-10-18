//! The text arm's embedder: all-MiniLM-L6-v2, 384 dimensions.
//!
//! One tower, one space. Every tile's text — a document chunk, an OCR line, a
//! file name — is embedded into the same 384-d space, so "the contract with the
//! non-compete" can find a lease that never says "non-compete".
//!
//! ## The pooling is the model
//!
//! A transformer exports a vector per token; turning that into one vector is a
//! decision, not a detail:
//!
//! - **Mean over the unmasked tokens.** This is the pooling the model card
//!   specifies (`mean_pooling(last_hidden_state, attention_mask)`), and padding
//!   positions are multiplied by zero rather than nudging the average.
//! - **Not the CLS token.** Token 0 is a token like any other in this family;
//!   reading it is the mistake the image arm's history already records once
//!   (a patch dressed up as an embedding). Two wrong ways to pool are not made
//!   right by being symmetric.
//!
//! ## The inputs are named in the graph
//!
//! The export takes `input_ids`, `attention_mask` and `token_type_ids`; the
//! names are read from the session rather than assumed, for the same reason as
//! the SigLIP towers: an export that reorders its inputs must fail at load, not
//! at the first query.
//!
//! ## The query is embedded as typed
//!
//! SigLIP's query normalisation (lower-case, strip a trailing period) was
//! measured on image captions and belongs to that arm. MiniLM's tokenizer is
//! uncased by design; copying the other arm's habit here would change what the
//! model was trained to see and is not something this code has measured.

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Context as _, Result};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

use crate::models::{set_version, Models};
use crate::store::TEXT_EMBEDDING_DIM;
use crate::vectors::Embedding;

/// The name of the model set these files come from.
pub const MODEL_SET: &str = "minilm";

/// The context length this arm uses, from the spike's configuration. Chunks are
/// cut well below this (200 words), so a tile is never truncated into meaning
/// something else; the cap is here so a pathological tile cannot turn into a
/// five-minute query.
const MAX_TOKENS: usize = 256;

/// The output that holds one vector per token.
const HIDDEN_OUTPUT: &str = "last_hidden_state";

/// The text arm: a MiniLM sentence encoder.
///
/// `Send + Sync` so one instance can be shared by the index sweep and the
/// query path, and by future index workers. The ONNX session is behind a lock
/// because a session is not safe for concurrent `run` — the same shape (and
/// the same acknowledged bottleneck) as the image arm.
pub struct MiniLm {
    session: Mutex<Session>,
    tokenizer: Arc<tokenizers::Tokenizer>,
    input_ids: String,
    attention_mask: String,
    token_type_ids: String,
    hidden_index: usize,
    /// The manifest version that produced this embedder. Recorded in `meta`
    /// only after a sweep that embedded everything.
    version: String,
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MiniLm>();
};

impl MiniLm {
    /// Load the pinned int8 model and its tokenizer, or fail with the command
    /// that fixes it.
    pub fn new(models: &Models, threads: usize) -> Result<Self> {
        let model_path = models.verified_path(MODEL_SET, "minilm.int8.onnx")?;
        let tokenizer_path = models.verified_path(MODEL_SET, "minilm-tokenizer.json")?;

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).map_err(|err| {
            anyhow!(
                "reading the MiniLM tokenizer at {}: {err}. Fix it with: tessera models install \
                 --set {MODEL_SET}",
                tokenizer_path.display()
            )
        })?;

        let environment = super::environment()?;
        let session = build_session(environment, &model_path, threads)?;

        let input_ids = input_name(&session, 0);
        let attention_mask = input_name(&session, 1);
        let token_type_ids = input_name(&session, 2);
        let hidden_index = output_index(&session, HIDDEN_OUTPUT)?;

        let set = models.manifest().set(MODEL_SET).ok_or_else(|| {
            anyhow!(
                "the model manifest has no {MODEL_SET:?} set, so the text arm cannot record which \
                 model built its vectors"
            )
        })?;

        Ok(Self {
            session: Mutex::new(session),
            tokenizer: Arc::new(tokenizer),
            input_ids,
            attention_mask,
            token_type_ids,
            hidden_index,
            version: set_version(set),
        })
    }

    /// Whether the arm's files are installed and verified. The cheap question,
    /// asked before a run so "why is meaning search empty" has an answer.
    pub fn is_installed(models: &Models) -> bool {
        models.verified_path(MODEL_SET, "minilm.int8.onnx").is_ok()
            && models
                .verified_path(MODEL_SET, "minilm-tokenizer.json")
                .is_ok()
    }

    /// The version token recorded in `meta` when this embedder fills the arm.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Embed one piece of text, normalized to unit length.
    ///
    /// `text` is embedded exactly as given: the tokenizer decides case and
    /// punctuation, and every caller here has either a stored tile or a user's
    /// query, both of which are the text the user actually has.
    pub fn embed_text(&self, text: &str) -> Result<Embedding> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|err| anyhow!("tokenising {text:?}: {err}"))?;

        let ids: Vec<i64> = encoding
            .get_ids()
            .iter()
            .take(MAX_TOKENS)
            .map(|&id| id as i64)
            .collect();
        let mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .take(MAX_TOKENS)
            .map(|&bit| bit as i64)
            .collect();
        if ids.is_empty() {
            bail!("the tokenizer produced no tokens for {text:?}");
        }
        let types = vec![0i64; ids.len()];
        let seq = ids.len();

        let input_ids =
            Tensor::from_array(([1usize, seq], ids.into_boxed_slice())).context("input_ids")?;
        let attention_mask =
            Tensor::from_array(([1usize, seq], mask.clone())).context("attention_mask")?;
        let token_type_ids = Tensor::from_array(([1usize, seq], types.into_boxed_slice()))
            .context("token_type_ids")?;

        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow!("the text arm's ONNX session was poisoned"))?;
        let outputs = session
            .run(vec![
                (self.input_ids.clone(), input_ids),
                (self.attention_mask.clone(), attention_mask),
                (self.token_type_ids.clone(), token_type_ids),
            ])
            .context("running the MiniLM encoder")?;

        let (shape, data) = outputs[self.hidden_index]
            .try_extract_tensor::<f32>()
            .context("reading the token embeddings")?;
        let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
        if dims.len() != 3 || dims[0] != 1 || dims[1] != seq || dims[2] != TEXT_EMBEDDING_DIM {
            bail!(
                "expected {HIDDEN_OUTPUT} of shape [1, {seq}, {TEXT_EMBEDDING_DIM}], got \
                 {dims:?}. The model may not be the one this code was written for."
            );
        }

        let pooled = mean_pool(data, &mask, TEXT_EMBEDDING_DIM)?;
        Ok(Embedding::new(pooled, TEXT_EMBEDDING_DIM)?.normalized())
    }

    /// Embed a user's query. The same tower as [`Self::embed_text`], named
    /// separately because this is the call the search path makes.
    pub fn embed_query(&self, text: &str) -> Result<Embedding> {
        self.embed_text(text)
    }
}

impl crate::vectors::TextEmbedder for MiniLm {
    fn version(&self) -> &str {
        &self.version
    }

    fn embed_text(&self, text: &str) -> Result<Embedding> {
        MiniLm::embed_text(self, text)
    }
}

/// Mean over the tokens whose mask is set, in flat `[seq, dim]` row-major form.
///
/// Padding is zeroed by the mask, not merely ignored: a padded token that slips
/// into the sum shifts the embedding toward whatever the padding happens to
/// encode, and the result still looks like a unit vector.
fn mean_pool(hidden: &[f32], mask: &[i64], dim: usize) -> Result<Vec<f32>> {
    if mask.is_empty() || hidden.len() != mask.len() * dim {
        bail!(
            "token embeddings are {} values for a {}-token mask and {dim} dimensions",
            hidden.len(),
            mask.len()
        );
    }
    let mut sums = vec![0f64; dim];
    let mut count = 0u64;
    for (token, &bit) in mask.iter().enumerate() {
        if bit == 0 {
            continue;
        }
        count += 1;
        let row = &hidden[token * dim..(token + 1) * dim];
        for (sum, value) in sums.iter_mut().zip(row) {
            *sum += f64::from(*value);
        }
    }
    if count == 0 {
        bail!("the attention mask selected no tokens");
    }
    Ok(sums
        .into_iter()
        .map(|sum| (sum / count as f64) as f32)
        .collect())
}

fn build_session(
    environment: &ort::environment::Environment,
    path: &Path,
    threads: usize,
) -> Result<Session> {
    // `ort::Error<SessionBuilder>` is neither Send nor Sync, so it is rendered
    // instead of wrapped.
    let builder = Session::builder(environment)
        .map_err(|err| anyhow!("{err}"))
        .and_then(|builder| {
            builder
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|err| anyhow!("{err}"))
        })
        .and_then(|builder| {
            builder
                .with_intra_threads(threads)
                .map_err(|err| anyhow!("{err}"))
        })
        .with_context(|| format!("configuring an ONNX session for {}", path.display()))?;
    let mut builder = builder;
    builder
        .commit_from_file(path)
        .with_context(|| format!("loading the ONNX model {}", path.display()))
}

fn input_name(session: &Session, index: usize) -> String {
    session
        .inputs()
        .get(index)
        .map(|input| input.name().to_string())
        .unwrap_or_default()
}

/// Where the token vectors live. Found by name at load time: an export with
/// several outputs is exactly where reading output 0 by position goes wrong.
fn output_index(session: &Session, name: &str) -> Result<usize> {
    let names: Vec<String> = session
        .outputs()
        .iter()
        .map(|output| output.name().to_string())
        .collect();
    names
        .iter()
        .position(|candidate| candidate == name)
        .ok_or_else(|| {
            anyhow!(
                "the model has no {name:?} output. Its outputs are {names:?}; this code expects \
                 the MiniLM export, and pooling a different output retrieves badly while looking \
                 correct."
            )
        })
}

#[cfg(test)]
mod tests {
    use super::mean_pool;

    #[test]
    fn padding_does_not_shift_the_mean() {
        // Three tokens of two dimensions; the third is padding.
        let hidden = [1.0, 2.0, 3.0, 4.0, 100.0, 100.0];
        let pooled = mean_pool(&hidden, &[1, 1, 0], 2).unwrap();
        assert_eq!(pooled, vec![2.0, 3.0]);
    }

    #[test]
    fn every_token_masked_is_an_error_not_a_zero_vector() {
        assert!(mean_pool(&[1.0, 2.0], &[0, 0], 2).is_err());
    }

    #[test]
    fn a_shape_mismatch_is_named() {
        let err = mean_pool(&[1.0, 2.0, 3.0], &[1, 1], 2).unwrap_err();
        assert!(err.to_string().contains("3 values"), "{err}");
    }
}
