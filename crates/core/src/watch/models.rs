//! What a drain loads, and when it lets it go.
//!
//! The product definition's memory rule is a ceiling on *idle* RAM (~250 MB),
//! not on the batch: models load when there is work and are dropped when the
//! queue drains. Two costs are deliberately separated:
//!
//! - **Verification** is the expensive, repeatable half — the pinned sets are
//!   about a gigabyte of SHA-256. It happens once per watch process, and the
//!   verified paths are reused.
//! - **Session construction** is the half that has to end with the drain. A
//!   session that is never dropped is the ~250 MB a machine at idle must not
//!   hold, so it is rebuilt from the already-verified paths on the next drain.
//!
//! A set that is not installed is not an error: the watcher keeps indexing text
//! formats and says once, at the first drain, that the arm is off. The
//! verification *answer* is cached, so a machine without the models does not
//! re-ask (and re-hash) on every batch.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use anyhow::Result;

use crate::index::docx::DocxPipeline;
use crate::index::image::ImagePipeline;
use crate::index::pdf::PdfPipeline;
use crate::index::text::TextPipeline;
use crate::index::Pipeline;
use crate::models::Models;
use crate::ocr::{self, onnx, OcrChoice, OcrEngine};
use crate::vectors::minilm::{self, MiniLm};
use crate::vectors::siglip::{self, SiglipEmbedder};
use crate::vectors::TextEmbedder;

/// One batch's worth of engine: the pipelines every worker shares, and the text
/// arm the coordinator sweeps with. Dropped whole when the backlog empties.
pub(crate) struct Drain {
    pub pipelines: Vec<Arc<dyn Pipeline>>,
    pub text: Option<Arc<dyn TextEmbedder>>,
    /// The version token for [`Drain::text`], recorded on the arm once a sweep
    /// has embedded every tile.
    pub text_version: Option<String>,
}

/// Where a drain's models come from. A trait so the watch loop can be tested
/// with a gated fake pipeline instead of a gigabyte of ONNX weights.
pub(crate) trait DrainModels: Send + Sync {
    fn load(&self) -> Result<Drain>;
}

/// The real thing: the pinned sets, verified once, sessions per drain.
pub(crate) struct EngineModels {
    models: Models,
    ocr: OcrChoice,
    threads: usize,
    image_paths: OnceLock<std::result::Result<(PathBuf, PathBuf, PathBuf), String>>,
    text_paths: OnceLock<std::result::Result<(PathBuf, PathBuf, String), String>>,
    said_image: OnceLock<()>,
    said_text: OnceLock<()>,
}

impl EngineModels {
    pub fn new(models: Models, ocr: OcrChoice, threads: usize) -> Self {
        Self {
            models,
            ocr,
            threads,
            image_paths: OnceLock::new(),
            text_paths: OnceLock::new(),
            said_image: OnceLock::new(),
            said_text: OnceLock::new(),
        }
    }

    fn image_paths(&self) -> &std::result::Result<(PathBuf, PathBuf, PathBuf), String> {
        self.image_paths.get_or_init(|| {
            siglip::SiglipEmbedder::verify_paths(&self.models).map_err(|e| format!("{e:#}"))
        })
    }

    fn text_paths(&self) -> &std::result::Result<(PathBuf, PathBuf, String), String> {
        self.text_paths
            .get_or_init(|| MiniLm::verify_paths(&self.models).map_err(|e| format!("{e:#}")))
    }

    /// The OCR engine for this drain.
    ///
    /// The native choice goes through `default_engine`, which keeps its
    /// platform-then-ONNX preference and its per-file `Unavailable` fallback. A
    /// user who *named* `onnx` gets the once-verified path instead, so the
    /// 23 MB set is not re-hashed per batch; a name that does not resolve is an
    /// error, exactly as it is for `tessera index`.
    fn ocr_engine(&self) -> Result<Box<dyn OcrEngine>> {
        match &self.ocr {
            OcrChoice::Native => Ok(ocr::default_engine(&self.models)),
            OcrChoice::Named(name) if name == "onnx" => {
                let (det, rec) = onnx::OnnxOcr::verify_paths(&self.models)?;
                Ok(Box::new(onnx::OnnxOcr::from_paths(
                    &det,
                    &rec,
                    self.threads,
                )?))
            }
            OcrChoice::Named(_) => Ok(ocr::select(&self.ocr, &self.models)?.engine),
        }
    }
}

impl DrainModels for EngineModels {
    fn load(&self) -> Result<Drain> {
        let engine = self.ocr_engine()?;

        let image = match self.image_paths() {
            Ok((vision, text, tokenizer)) => {
                match SiglipEmbedder::from_paths(vision, text, tokenizer, self.threads) {
                    Ok(embedder) => Some(Arc::new(embedder)),
                    Err(err) => {
                        say_once(&self.said_image, || {
                            format!(
                                "the siglip models are present but the image arm would not load, so \
                                 description search is off for this run: {err:#}"
                            )
                        });
                        None
                    }
                }
            }
            Err(message) => {
                // A missing set is the normal state on a fresh machine: silence.
                // Present-but-unverifiable is worth saying once.
                if set_present(&self.models, siglip::MODEL_SET) {
                    say_once(&self.said_image, || message.clone());
                }
                None
            }
        };

        let (text, text_version) = match self.text_paths() {
            Ok((model, tokenizer, version)) => {
                match MiniLm::from_paths(model, tokenizer, version, self.threads) {
                    Ok(embedder) => (
                        Some(Arc::new(embedder) as Arc<dyn TextEmbedder>),
                        Some(version.clone()),
                    ),
                    Err(err) => {
                        say_once(&self.said_text, || {
                            format!(
                                "the minilm models are present but the meaning arm would not load, so \
                                 meaning search is off for this run: {err:#}"
                            )
                        });
                        (None, None)
                    }
                }
            }
            Err(message) => {
                if set_present(&self.models, minilm::MODEL_SET) {
                    say_once(&self.said_text, || message.clone());
                }
                (None, None)
            }
        };

        let pipelines: Vec<Arc<dyn Pipeline>> = vec![
            Arc::new(TextPipeline),
            Arc::new(PdfPipeline),
            Arc::new(DocxPipeline),
            Arc::new(ImagePipeline::new(engine).with_embedder(image)),
        ];

        Ok(Drain {
            pipelines,
            text,
            text_version,
        })
    }
}

/// Say `message` at most once per process. `slot` is the "have I said it" flag;
/// the first caller wins and every later one is silent.
fn say_once(slot: &OnceLock<()>, message: impl FnOnce() -> String) {
    if slot.set(()).is_ok() {
        eprintln!("{}", message());
    }
}

/// Whether every file of a set is at least on disk — presence, not
/// verification. Used only to decide between silence and a warning, and
/// re-hashing a gigabyte to phrase a warning is how the warning becomes worse
/// than the problem.
fn set_present(models: &Models, set: &str) -> bool {
    models
        .manifest()
        .set(set)
        .is_some_and(|set| set.files.iter().all(|file| models.path_of(file).exists()))
}
