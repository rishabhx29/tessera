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

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
/// A size/quality trade, not a correctness switch. An earlier version of this
/// comment claimed that int8 "compresses the embedding space" and that fp32 had
/// "usable dynamic range", on the evidence that two unrelated screenshots sit at
/// a median cosine of 0.51. That reasoning was wrong twice over: five-tenths
/// between unrelated images is **anisotropy**, which every healthy transformer
/// embedding has and which a corpus of screenshots exaggerates, and a cosine
/// floor is not a measure of retrieval quality in any case. Ranking is.
/// `docs/performance.md` carries the correction.
///
/// Ranking is what was actually traded off, and the two precisions are close
/// enough on it that the 213 MB file is what ships. The fp32 files stay pinned so
/// that the comparison remains runnable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// int8: 213 MB total. What `new()` loads and what the manifest installs.
    Int8,
    /// float32: 776 MB total. Kept for the precision comparison, not installed
    /// by default and not read by the engine.
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
        let (vision_path, text_path, tokenizer_path) = Self::verify_paths_with(models, precision)?;
        Self::from_paths(&vision_path, &text_path, &tokenizer_path, threads)
    }

    /// The three verified paths of the pinned int8 set.
    ///
    /// Verification is the expensive half — about a gigabyte of SHA-256 — so a
    /// caller that builds an embedder per batch (the watcher) does it once here
    /// and hands the paths to [`SiglipEmbedder::from_paths`].
    pub fn verify_paths(models: &Models) -> Result<(PathBuf, PathBuf, PathBuf)> {
        Self::verify_paths_with(models, Precision::Int8)
    }

    fn verify_paths_with(
        models: &Models,
        precision: Precision,
    ) -> Result<(PathBuf, PathBuf, PathBuf)> {
        let (vision_file, text_file) = precision.files();
        // One verification pass for all three files: each `verified_path`
        // call re-hashes the whole set, and the set is a gigabyte.
        let paths = models.verified_paths(
            MODEL_SET,
            &[vision_file, text_file, "siglip-tokenizer.json"],
        )?;
        let [vision_path, text_path, tokenizer_path] = paths
            .try_into()
            .expect("verified_paths returned one path per requested file");
        Ok((vision_path, text_path, tokenizer_path))
    }

    /// Load the two towers and the tokenizer from paths that have **already**
    /// been verified.
    ///
    /// The watcher verifies the pinned set once per process and rebuilds
    /// sessions per drain from these paths: a constructor that re-hashes
    /// 1 GB of weights per batch would cost more than the batch, and a session
    /// that is never dropped is the ~250 MB a machine at idle must not hold.
    pub fn from_paths(
        vision_path: &Path,
        text_path: &Path,
        tokenizer_path: &Path,
        threads: usize,
    ) -> Result<Self> {
        let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_path).map_err(|err| {
            anyhow::anyhow!(
                "reading the SigLIP tokenizer at {}: {err}. Fix it with: tessera models install \
                 --set {MODEL_SET}",
                tokenizer_path.display()
            )
        })?;

        let environment = super::environment()?;
        let vision = build_session(environment, vision_path, threads)?;
        let text = build_session(environment, text_path, threads)?;

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
    ///
    /// One set verification, not three: each per-file check hashes every file
    /// in the set, and the siglip set — both precisions included — is about a
    /// gigabyte.
    pub fn is_installed(models: &Models) -> bool {
        models.is_set_ready(MODEL_SET)
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

    /// Embed text exactly as given, with no query normalisation.
    ///
    /// Benchmarks and tests use this to measure the tower itself. A search path
    /// wants [`Self::embed_query`], which is the form the numbers describe.
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

    /// Embed a user's query.
    ///
    /// The text is normalised first — see [`normalise_query`] for what that does
    /// and which measurements it comes from. This is the method a search box
    /// calls; `embed_text` embeds exactly what it is handed, which is what a
    /// benchmark wants and not what a query path wants.
    pub fn embed_query(&self, text: &str, dim: usize) -> Result<Embedding> {
        self.embed_text(&normalise_query(text), dim)
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

impl crate::vectors::QueryEmbedder for SiglipEmbedder {
    fn dim(&self) -> usize {
        crate::store::IMAGE_EMBEDDING_DIM
    }

    fn embed_query(&self, text: &str) -> Result<Embedding> {
        SiglipEmbedder::embed_query(self, text, crate::store::IMAGE_EMBEDDING_DIM)
    }
}

/// Put a query into the form this arm's numbers were measured on.
///
/// Image captions in the wild are lower case, and the tower was trained against
/// them; an annotator's sentence is off-distribution for it. Over the
/// 300-photograph corpus with all 1501 captions as queries, lower-casing the
/// query moved R@1 from 0.6629 to 0.6962, and dropping a trailing period took it
/// to 0.7075 — R@5 from 0.9121 to 0.9420.
///
/// The half of that which needed proving is the opposite: that lower-casing does
/// not damage the queries which *carry* capitals. It does the reverse. On the 61
/// captions with a capital after their first character — proper nouns, brand
/// names, school names — R@1 went from 0.3770 to 0.7705 and mean rank from 33.4
/// to 2.7. Those are the off-distribution queries, and this pulls them in rather
/// than erasing them.
///
/// It is the identity on the shape a user actually types: a lower-case fragment
/// with no final period. It therefore cannot help those, and cannot hurt them
/// either — every point of the measured gain is on sentence-shaped input. That is
/// stated plainly because a reader who assumes `--describe "a dog"` got faster
/// has been misled by a number taken on sentences.
///
/// Commas are left alone deliberately: stripping them measured +0.0014 R@1,
/// which is two queries of 1501, and it is not worth changing behaviour for.
pub fn normalise_query(text: &str) -> String {
    text.trim().trim_end_matches('.').trim_end().to_lowercase()
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

#[cfg(test)]
mod tests {
    use super::normalise_query;

    #[test]
    fn a_sentence_becomes_what_the_tower_was_trained_on() {
        assert_eq!(
            normalise_query("A woman stands in the dining area at the table."),
            "a woman stands in the dining area at the table"
        );
    }

    #[test]
    fn a_typed_fragment_is_the_identity() {
        // The common case, and the reason this transform costs nothing: a
        // fragment a user types is already in the form the model wants. If this
        // ever stops being the identity, the change is no longer free for the
        // shape of query the product actually receives.
        for query in [
            "a dog running through grass",
            "a close up of a cat face",
            "a city skyline at night",
            "a mountain range at sunrise",
        ] {
            assert_eq!(normalise_query(query), query);
        }
    }

    #[test]
    fn interior_punctuation_survives() {
        // Stripping commas measured +0.0014 R@1 - two queries of 1501, which is
        // noise. This pins the decision rather than the number: deleting
        // punctuation the user typed, for two queries in 1501, is not a trade
        // worth making.
        assert_eq!(
            normalise_query("a dog, sitting, on grass"),
            "a dog, sitting, on grass"
        );
        assert_eq!(normalise_query("Dr. Smith's car."), "dr. smith's car");
    }

    #[test]
    fn whitespace_goes_from_the_ends_and_nowhere_else() {
        assert_eq!(normalise_query("  a dog\n"), "a dog");
        assert_eq!(normalise_query("a  dog"), "a  dog");
    }

    #[test]
    fn a_query_of_punctuation_alone_becomes_empty() {
        // Empty is a strange query rather than a crash - the tokeniser encodes
        // "" as the end-of-text token on its own. Asserted because `embed_query`
        // can turn a non-empty input into an empty one, and that is the sort of
        // thing worth being visible rather than discovered.
        assert_eq!(normalise_query("..."), "");
    }
}
