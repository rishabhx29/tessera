//! The cross-platform OCR engine: PP-OCRv6 detection + recognition on ONNX
//! Runtime (ADR-0008).
//!
//! This is the fallback that makes OCR work on Linux, and the quality pass on
//! every platform. It is also the slow one. Measured over this machine's
//! 40-screenshot corpus, one image at a time: the native Windows engine p50
//! **34.4 ms** extracting 188 characters, this one p50 **949.6 ms** extracting
//! **288** — roughly 28× the time for 53% more text, and the one image in the
//! corpus the native engine found no text in at all is read here. Hardware,
//! method and caveats in `docs/performance.md`; the comparison from
//! `tessera ocr <image> --engine onnx --scores`.
//!
//! ## What this module is responsible for
//!
//! Decoding the file, running two graphs, and turning numbers into lines. Every
//! rule that came out of the spike's debug loop lives in [`charset`] and
//! [`layout`] as a testable function; what is left here is the wiring.
//!
//! ## Thread safety
//!
//! `OcrEngine` is `Send + Sync`, and ONNX Runtime sessions are explicitly *not*
//! safe for concurrent `run`. Sessions are therefore handed out from a free list
//! rather than shared: two threads recognise at once only because there are two
//! sessions, created on demand. A single session under a lock would be simpler
//! and would also serialise the whole engine to one image at a time, which is the
//! opposite of what the index queue in ticket 11 needs.

pub mod charset;
pub mod layout;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{bail, Context, Result};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::{Tensor, ValueType};

use super::{lines_into_text, OcrEngine};
use crate::models::Models;

/// The model set the engine needs. One name, so "are the OCR models installed?"
/// is one question with one answer.
pub const MODEL_SET: &str = "ppocr";

/// The two files of the set, named where a caller can verify them once and
/// build the engine from the paths later.
pub const DET_FILE: &str = "ppocrv6-det-tiny.onnx";
pub const REC_FILE: &str = "ppocrv6-rec-small.onnx";

/// Detection runs on an image scaled down so its longest side is at most this.
///
/// Downscale only: never enlarge, because the detector was trained at native
/// scale and upscaling a small screenshot invents detail it has no basis for.
const DET_MAX_SIDE: u32 = 1280;

/// Both detection sides are rounded to this. The graph resizes internally by 1/32
/// and refuses an input that is not divisible by it.
const DET_MULTIPLE: u32 = 32;

/// A probability above this is text. The spike's value; below it the blobs are
/// mostly separators and icons.
const DET_THRESHOLD: f32 = 0.3;

/// Recognition confidence below this is dropped.
///
/// The 0.5 is what the ticket specifies and what the corpus was measured with.
/// It does **not** separate the two populations on confidence alone: real text
/// reads 0.98–1.00 and icon misreads reach 0.73, which is above the gate. What
/// actually drops them is the length check beside this, because the misreads
/// observed were short. A gate in (0.73, 0.98) would separate them on confidence
/// alone; whether that costs real characters is an open question for the quality
/// pass, recorded with its reasoning in ADR-0008 and in `charset.rs`.
///
/// Public because it is a tuning knob a test asserts against, not a private
/// constant that can drift silently away from the behaviour it describes.
pub const SCORE_GATE: f32 = 0.5;

/// A line must be longer than this to be text. One character is nearly always an
/// icon misread that cleared the gate by luck.
pub const MIN_LINE_CHARS: usize = 1;

/// Padding added around a detected box before cropping. Glyphs overshoot their
/// component's bounding box by a pixel or two; cropping tight loses ascenders.
const CROP_PAD: u32 = 2;

const REC_MIN_WIDTH: u32 = 16;
const REC_MAX_WIDTH: u32 = 1920;

/// Total pixels of line width per recognition batch.
///
/// The recogniser's output is `[batch, steps, 18,710]`, so this tensor dominates
/// both memory and time. Batching by count instead of by width is what makes the
/// engine slow (ADR-0008).
const BATCH_MAX_WIDTH: usize = 9_600;

/// Lines per recognition batch, as a second ceiling.
const BATCH_MAX_LINES: usize = 24;

/// Detection normalisation: the ImageNet mean/std the detector was trained with.
const DET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DET_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// The ONNX engine.
///
/// Cheap to clone — it shares one set of models and sessions — so a caller that
/// wants two can hold two without loading the 23 MB of weights twice.
#[derive(Clone)]
pub struct OnnxOcr {
    inner: Arc<Inner>,
}

struct Inner {
    det_path: PathBuf,
    rec_path: PathBuf,
    det_input: String,
    rec_input: String,
    /// Idle detection sessions, ready to be taken.
    det: Mutex<VecDeque<Session>>,
    rec: Mutex<VecDeque<Session>>,
    alphabet: Vec<String>,
    rec_height: u32,
    threads: usize,
}

/// ONNX Runtime allows **exactly one environment per process**, so it is created
/// once and shared by every session this module builds.
///
/// Calling `ort::init()` per engine looks harmless and is not: the second call
/// fails outright with "only one environment is allowed per process". Not
/// theoretical — the CLI builds an engine per command, the index queue will build
/// one per worker, and a test process builds one per test. Found by running the
/// integration tests in parallel, which is what tests are for.
fn environment() -> Result<&'static ort::environment::Environment> {
    // The error is kept as a string because `ort`'s error type is not `Send`, and
    // a `OnceLock` shared across threads has to hold something that is.
    static ENVIRONMENT: OnceLock<std::result::Result<ort::environment::Environment, String>> =
        OnceLock::new();
    let built = ENVIRONMENT.get_or_init(|| {
        let loaded = loaded_runtime_version();
        // `ort` does not return an error when the loaded runtime is older than
        // the API level this build asks for: `setup_api` panics with "Failed to
        // initialize ORT API" when `GetApi(23)` comes back null, naming neither
        // the version nor the file. Catch the panic so the refusal can name the
        // runtime that actually answered.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ort::init().build())) {
            Ok(Ok(environment)) => Ok(environment),
            Ok(Err(err)) => Err(refusal_message(
                &format!("{err}"),
                loaded.as_deref(),
                cfg!(windows),
            )),
            Err(_) => Err(refusal_message(
                "failed to initialize the ORT API",
                loaded.as_deref(),
                cfg!(windows),
            )),
        }
    });
    match built {
        Ok(environment) => Ok(environment),
        Err(message) => bail!("{message}"),
    }
}

/// The error `environment` reports when the runtime will not start.
///
/// `ort`'s own failure is a bare panic — "Failed to initialize ORT API" — which
/// says neither that the API level is the problem nor which runtime answered.
/// The actionable half is the version: this build asks for ORT API 23
/// (onnxruntime 1.23), and an older runtime refuses it. On Windows the older
/// runtime is usually not the one that was shipped: `onnxruntime.dll` is
/// resolved through the DLL search order, so a build whose 1.23 dll is not
/// beside the executable silently binds to the 1.17.1 copy Windows keeps in
/// `System32`. Naming what was loaded and the fix turns a mysterious panic into
/// a packaging bug someone can act on.
///
/// A pure function of the probe's answer, so both branches are exercised on any
/// host rather than only where the failure occurs.
fn refusal_message(detail: &str, loaded: Option<&str>, windows: bool) -> String {
    let mut message = format!(
        "starting ONNX Runtime: {detail}. This build asks for ORT API 23 (onnxruntime 1.23); \
         an older runtime does not answer it."
    );
    match loaded {
        Some(version) => message.push_str(&format!(
            " The onnxruntime.dll loaded into this process reports version {version}."
        )),
        None => message.push_str(" The loaded onnxruntime.dll did not report its version."),
    }
    if windows {
        message.push_str(
            " Windows resolves onnxruntime.dll through the DLL search order, so a build whose \
             own 1.23 dll is not beside the executable binds to the 1.17.1 copy in System32, \
             which is too old: put the 1.23 onnxruntime.dll next to the executable.",
        );
    }
    message
}

/// The version string of the ONNX Runtime this process actually loaded, or `None`
/// when it cannot be read.
///
/// The platform fact — and the reason it is a separate adapter — lives in
/// `ocr::ort_dll`: Windows resolves `onnxruntime.dll` through the DLL search
/// order and can bind an older copy than the one this build ships.
fn loaded_runtime_version() -> Option<String> {
    super::ort_dll::loaded_runtime_version()
}

impl OnnxOcr {
    /// Build the engine from the installed, verified `ppocr` models.
    ///
    /// Fails rather than falling back to something weaker: an engine that
    /// half-loads produces text-shaped noise, and the pipeline would index it.
    pub fn new(models: &Models) -> Result<Self> {
        let (det_path, rec_path) = Self::verify_paths(models)?;

        // A sensible default for a CPU-bound recogniser; ONNX Runtime would
        // otherwise spawn a thread per core and fight the index queue for them.
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 8);

        Self::from_paths(&det_path, &rec_path, threads)
    }

    /// The two verified model paths, in detection/recognition order.
    ///
    /// One verification pass for both files: each per-file check re-hashes the
    /// whole set, and a caller that builds an engine per batch (the watcher)
    /// should not pay that per batch.
    pub fn verify_paths(models: &Models) -> Result<(PathBuf, PathBuf)> {
        let paths = models.verified_paths(MODEL_SET, &[DET_FILE, REC_FILE])?;
        let [det_path, rec_path] = paths
            .try_into()
            .expect("verified_paths returned one path per requested file");
        Ok((det_path, rec_path))
    }

    /// Build the engine from paths that have **already** been verified.
    ///
    /// The watcher verifies the pinned set once per process and rebuilds
    /// sessions per drain from these paths; verification is the expensive,
    /// repeatable half and session construction is the half that must end when
    /// the queue drains.
    pub fn from_paths(det_path: &Path, rec_path: &Path, threads: usize) -> Result<Self> {
        let environment = environment()?;
        // One session up front so the first image does not pay session
        // construction, and so a broken model file is reported at startup rather
        // than on the hundredth screenshot.
        let det = build_session(environment, det_path, threads)?;
        let rec = build_session(environment, rec_path, threads)?;

        let det_input = det.inputs()[0].name().to_string();
        let rec_input = rec.inputs()[0].name().to_string();
        let rec_height = input_height(&rec)?;

        let alphabet = charset::alphabet(
            &rec.metadata()
                .context("reading the recogniser's metadata")?
                .custom("character")
                .context(
                    "the recogniser model has no `character` metadata, so its output cannot \
                          be mapped to characters",
                )?,
        );

        // The single most useful guard in this engine: the table's length must be
        // the graph's output width, or every class index is wrong by some offset
        // and the output is fluent nonsense. Fail here, loudly, instead.
        let classes = output_classes(&rec)?;
        if alphabet.len() != classes {
            bail!(
                "the recogniser emits {classes} classes but its `character` metadata describes \
                 {}; the two must match or every character is misread",
                alphabet.len()
            );
        }

        Ok(Self {
            inner: Arc::new(Inner {
                det_path: det_path.to_path_buf(),
                rec_path: rec_path.to_path_buf(),
                det_input,
                rec_input,
                det: Mutex::new(VecDeque::from([det])),
                rec: Mutex::new(VecDeque::from([rec])),
                alphabet,
                rec_height,
                threads,
            }),
        })
    }

    /// Whether the models this engine needs are installed and verified, without
    /// paying to load them.
    pub fn is_installed(models: &Models) -> bool {
        models.is_set_ready(MODEL_SET)
    }

    /// One line of text and the confidence behind it, for the comparison command
    /// and for diagnostics.
    pub fn lines(&self, path: &Path) -> Result<Vec<(String, f32)>> {
        let image = decode(path)?;
        let boxes = self.detect(&image)?;
        let mut found = self.recognize(&image, &boxes)?;
        // Reading order is what a person means by "the text of this screenshot".
        // The boxes come out of a flood fill in raster order, which on a
        // two-column screenshot is the wrong one.
        found.sort_by_key(|(bbox, _)| (bbox.y0, bbox.x0));
        Ok(found
            .into_iter()
            .map(|(_, read)| (read.text, read.score))
            .collect())
    }

    /// Every text region in the image, as boxes in the original image's pixels.
    fn detect(&self, image: &image::RgbImage) -> Result<Vec<layout::Box>> {
        let mut leased = Leased::take(&self.inner.det, || {
            environment()
                .and_then(|env| build_session(env, &self.inner.det_path, self.inner.threads))
        })?;
        let boxes = self.detect_with(image, leased.get());
        leased.give_back();
        boxes
    }

    fn detect_with(
        &self,
        image: &image::RgbImage,
        session: &mut Session,
    ) -> Result<Vec<layout::Box>> {
        let (width, height) = (image.width(), image.height());
        let (target_w, target_h) = layout::det_size(width, height, DET_MAX_SIDE, DET_MULTIPLE);
        let resized = if (target_w, target_h) == (width, height) {
            image.clone()
        } else {
            // Triangle filtering is bilinear with the kernel widened for
            // downscaling, which is the antialiasing that keeps thin strokes
            // detectable. Nearest-neighbour here loses whole glyphs.
            image::imageops::resize(
                image,
                target_w,
                target_h,
                image::imageops::FilterType::Triangle,
            )
        };

        let tensor = Tensor::from_array(([1usize, 3, target_h as usize, target_w as usize], {
            let mut buffer = vec![0f32; 3 * target_h as usize * target_w as usize];
            let plane = target_h as usize * target_w as usize;
            for (index, pixel) in resized.pixels().enumerate() {
                for channel in 0..3 {
                    buffer[channel * plane + index] =
                        (pixel[channel] as f32 / 255.0 - DET_MEAN[channel]) / DET_STD[channel];
                }
            }
            buffer
        }))
        .context("building the detection input tensor")?;

        let outputs = session.run(vec![(self.inner.det_input.clone(), tensor)])?;
        let (shape, probabilities) = outputs[0]
            .try_extract_tensor::<f32>()
            .context("reading the detection output")?;
        let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
        // [1, 1, H, W]: assert the rank rather than trusting it, because every
        // indexing decision below assumes it.
        if dims.len() != 4 || dims[2] != target_h as usize || dims[3] != target_w as usize {
            bail!("the detector returned shape {dims:?}, expected [1, 1, {target_h}, {target_w}]");
        }

        let mask = layout::threshold(
            probabilities,
            target_w as usize,
            target_h as usize,
            DET_THRESHOLD,
        );
        let components = layout::components(&mask, target_w as usize, target_h as usize);
        let scale_x = target_w as f32 / width as f32;
        let scale_y = target_h as f32 / height as f32;
        Ok(layout::plausible(&components)
            .into_iter()
            .map(|bbox| bbox.scaled_back(scale_x, scale_y, width, height))
            .collect())
    }

    /// Read every box, batching by total width.
    fn recognize(
        &self,
        image: &image::RgbImage,
        boxes: &[layout::Box],
    ) -> Result<Vec<(layout::Box, charset::Read)>> {
        // Crop, pad and resize every box first, so the widths are known before
        // anything is batched.
        let mut lines: Vec<(layout::Box, image::RgbImage, usize)> = Vec::new();
        for &bbox in boxes {
            let Some(crop) = crop(image, bbox) else {
                continue;
            };
            let Some((w, h)) = layout::rec_size(
                crop.width(),
                crop.height(),
                self.inner.rec_height,
                REC_MIN_WIDTH,
                REC_MAX_WIDTH,
            ) else {
                continue;
            };
            let resized =
                image::imageops::resize(&crop, w, h, image::imageops::FilterType::Triangle);
            lines.push((bbox, resized, w as usize));
        }
        if lines.is_empty() {
            return Ok(Vec::new());
        }

        let widths: Vec<usize> = lines.iter().map(|(_, _, w)| *w).collect();
        let batches = layout::batches_by_width(&widths, BATCH_MAX_WIDTH, BATCH_MAX_LINES);

        let mut reads = vec![None; lines.len()];
        // One lease for the whole set of batches: a session per batch would mean
        // rebuilding a 23 MB session dozens of times per image.
        let mut leased = Leased::take(&self.inner.rec, || {
            environment()
                .and_then(|env| build_session(env, &self.inner.rec_path, self.inner.threads))
        })?;
        let outcome = (|| {
            for batch in batches {
                let widest = batch.iter().map(|i| widths[*i]).max().unwrap_or(1);
                let height = self.inner.rec_height as usize;
                let plane = 3 * height * widest;
                let mut buffer = vec![0f32; batch.len() * plane];
                for (slot, &index) in batch.iter().enumerate() {
                    let base = slot * plane;
                    // `image` is RGB and its rows are contiguous, so this is a copy
                    // with the channel axis moved to the front and the /255, -0.5,
                    // /0.5 normalisation folded in.
                    let (_, source, _) = &lines[index];
                    for y in 0..height {
                        for x in 0..widths[index] {
                            let pixel = source.get_pixel(x as u32, y as u32);
                            for channel in 0..3 {
                                buffer[base + channel * height * widest + y * widest + x] =
                                    (pixel[channel] as f32 / 255.0 - 0.5) / 0.5;
                            }
                        }
                    }
                }

                let tensor = Tensor::from_array(([batch.len(), 3, height, widest], buffer))
                    .context("building a recognition input tensor")?;
                let outputs = leased
                    .get()
                    .run(vec![(self.inner.rec_input.clone(), tensor)])?;
                let (shape, values) = outputs[0]
                    .try_extract_tensor::<f32>()
                    .context("reading the recognition output")?;
                let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
                if dims.len() != 3 || dims[0] != batch.len() {
                    bail!(
                        "the recogniser returned shape {dims:?}, expected [{}, steps, classes]",
                        batch.len()
                    );
                }
                let (steps, classes) = (dims[1], dims[2]);

                for (slot, &index) in batch.iter().enumerate() {
                    let base = slot * steps * classes;
                    let mut steps_best = Vec::with_capacity(steps);
                    let mut scores = Vec::with_capacity(steps);
                    for step in 0..steps {
                        let row = &values[base + step * classes..base + (step + 1) * classes];
                        let (best, score) = argmax(row);
                        steps_best.push(best);
                        scores.push(score);
                    }
                    reads[index] = Some(charset::greedy_decode(
                        &steps_best,
                        &scores,
                        &self.inner.alphabet,
                    ));
                }
            }
            Ok(())
        })();
        leased.give_back();
        outcome?;

        // The gate, and only the gate, decides what is text.
        Ok(lines
            .into_iter()
            .zip(reads)
            .filter_map(|((bbox, _, _), read)| {
                let read = read?;
                if !charset::passes_gate(&read.text, read.score, SCORE_GATE, MIN_LINE_CHARS) {
                    return None;
                }
                Some((
                    bbox,
                    charset::Read {
                        text: read.text.trim().to_string(),
                        score: read.score,
                    },
                ))
            })
            .collect())
    }
}

/// Borrow a session, use it, and put it back whatever happens.
///
/// Without this a failed run would permanently shrink the pool, and a long
/// indexing session would end up building a fresh 23 MB session per image.
struct Leased<'a> {
    pool: &'a Mutex<VecDeque<Session>>,
    session: Option<Session>,
}

impl<'a> Leased<'a> {
    /// Take a session from the free list, or make a new one.
    ///
    /// Creating on demand rather than all up front: how many are needed depends
    /// on how many threads the caller indexes with, and paying for eight 23 MB
    /// sessions on a one-thread run would be a waste nobody asked for.
    fn take(
        pool: &'a Mutex<VecDeque<Session>>,
        fresh: impl FnOnce() -> Result<Session>,
    ) -> Result<Self> {
        let session = match pool.lock().expect("session pool").pop_front() {
            Some(session) => session,
            None => fresh()?,
        };
        Ok(Self {
            pool,
            session: Some(session),
        })
    }

    fn get(&mut self) -> &mut Session {
        self.session
            .as_mut()
            .expect("a leased session is always held")
    }

    fn give_back(self) {
        if let Some(session) = self.session {
            self.pool.lock().expect("session pool").push_back(session);
        }
    }
}

impl OcrEngine for OnnxOcr {
    fn name(&self) -> &'static str {
        "onnx"
    }

    fn recognize(&self, path: &Path) -> Result<String> {
        let lines: Vec<String> = self
            .lines(path)?
            .into_iter()
            .map(|(text, _)| text)
            .collect();
        Ok(lines_into_text(&lines))
    }

    fn lines_with_scores(&self, path: &Path) -> Result<Vec<(String, Option<f32>)>> {
        Ok(self
            .lines(path)?
            .into_iter()
            .map(|(text, score)| (text, Some(score)))
            .collect())
    }
}

fn build_session(
    environment: &ort::environment::Environment,
    path: &Path,
    threads: usize,
) -> Result<Session> {
    // `ort::Error<SessionBuilder>` is neither Send nor Sync, so it cannot become
    // an `anyhow::Error`; it is rendered instead.
    let mut builder = ort::session::Session::builder(environment)
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
    builder
        .commit_from_file(path)
        .with_context(|| format!("loading the ONNX model {}", path.display()))
}

/// The image as RGB, whatever format it was stored in.
///
/// Decoding is this engine's own business — it is a library engine with no
/// operating system to ask, which is exactly the reason the seam takes a path.
fn decode(path: &Path) -> Result<image::RgbImage> {
    // Canonicalise first: `image::open` reports a missing file as "No such file"
    // without ever naming which one, and a walk hands out relative paths.
    let path = dunce::canonicalize(path).with_context(|| format!("decoding {}", path.display()))?;
    let image = image::open(&path)
        .with_context(|| format!("decoding {} as an image", path.display()))?
        .to_rgb8();
    Ok(image)
}

/// A crop of `image` around `bbox`, padded, or `None` if it would be degenerate.
fn crop(image: &image::RgbImage, bbox: layout::Box) -> Option<image::RgbImage> {
    let (width, height) = (image.width(), image.height());
    let x0 = bbox.x0.saturating_sub(CROP_PAD);
    let y0 = bbox.y0.saturating_sub(CROP_PAD);
    let x1 = bbox.x1.saturating_add(CROP_PAD).min(width);
    let y1 = bbox.y1.saturating_add(CROP_PAD).min(height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(image::imageops::crop_imm(image, x0, y0, x1 - x0, y1 - y0).to_image())
}

/// The recogniser's input height, read from the graph rather than assumed.
fn input_height(session: &Session) -> Result<u32> {
    let ValueType::Tensor { shape, .. } = &session.inputs()[0].dtype() else {
        bail!("the recognition model does not take a tensor");
    };
    // [batch, 3, height, width]: index 2 is the height, and it must be static.
    let height = shape.get(2).copied().unwrap_or(-1);
    if height <= 0 {
        bail!(
            "the recognition model's input height is dynamic ({height}), which this engine \
               does not know how to size lines for"
        );
    }
    Ok(height as u32)
}

/// The recogniser's output class count, read from the graph.
fn output_classes(session: &Session) -> Result<usize> {
    let ValueType::Tensor { shape, .. } = &session.outputs()[0].dtype() else {
        bail!("the recognition model does not produce a tensor");
    };
    // [batch, steps, classes]: index 2 is the class count, and it must be static.
    let classes = shape.get(2).copied().unwrap_or(-1);
    if classes <= 0 {
        bail!(
            "the recognition model's class count is dynamic ({classes}), which this engine \
               cannot map to characters"
        );
    }
    Ok(classes as usize)
}

/// The best class in a row of scores, and its value.
fn argmax(row: &[f32]) -> (usize, f32) {
    let mut best = 0usize;
    let mut score = f32::NEG_INFINITY;
    for (index, &value) in row.iter().enumerate() {
        if value > score {
            best = index;
            score = value;
        }
    }
    (best, if score.is_finite() { score } else { 0.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The refusal is the one error a user cannot debug from ort's message, so
    // what it names is the whole point. These pin the two facts that make it
    // actionable — the version that answered, and where the wrong dll comes
    // from — without needing a broken runtime to reproduce it.

    #[test]
    fn a_refusal_names_the_loaded_runtime() {
        let message = refusal_message("failed to initialize the ORT API", Some("1.17.1"), true);
        assert!(
            message.contains("1.17.1"),
            "the version that actually answered must be in the message: {message}"
        );
        assert!(
            message.contains("onnxruntime.dll"),
            "the message must name the file to replace: {message}"
        );
    }

    #[test]
    fn a_windows_refusal_points_at_the_system32_fallback() {
        let with_version = refusal_message("boom", Some("1.17.1"), true);
        for expected in ["System32", "1.23", "beside the executable"] {
            assert!(
                with_version.contains(expected),
                "{expected:?} must appear so the fix is obvious: {with_version}"
            );
        }
        // The probe failing to read a version must not lose the hint either:
        // "no version" on Windows is exactly the missing-bundled-dll case.
        let without_version = refusal_message("boom", None, true);
        assert!(without_version.contains("System32"), "{without_version}");
        assert!(
            without_version.contains("did not report"),
            "{without_version}"
        );
    }

    #[test]
    fn a_non_windows_refusal_does_not_talk_about_system32() {
        let message = refusal_message("boom", Some("1.17.1"), false);
        assert!(message.contains("1.17.1"), "{message}");
        assert!(
            !message.contains("System32"),
            "System32 is a Windows search-path fact: {message}"
        );
    }
}
