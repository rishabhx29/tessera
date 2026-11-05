//! The `tessera` command-line interface: index, search, stats.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::fusion::{parse_query, search as fused_search};
use crate::index::{
    docx::DocxPipeline, image::ImagePipeline, index_folder, index_folder_with_text,
    pdf::PdfPipeline, text::TextPipeline, Pipeline,
};
use crate::models::Models;
use crate::ocr::{self, OcrChoice, OcrEngine};
use crate::search::snippet_around;
use crate::store::{Arm, Store, IMAGE_EMBEDDING_DIM, TEXT_EMBEDDING_DIM};
use crate::vectors::minilm::{self, MiniLm};
use crate::vectors::siglip::{self, SiglipEmbedder};
use crate::vectors::{QueryEmbedder, TextEmbedder};

/// Where the models live: the override if given, otherwise a `models` folder
/// beside the index. One designated directory to back up, and one to delete.
pub fn models_dir(data_dir: &Path, override_dir: Option<&Path>) -> PathBuf {
    match override_dir {
        Some(dir) => dir.to_path_buf(),
        None => data_dir.join("models"),
    }
}

/// Bytes, said the way a person reading a setup screen would want them.
fn human_bytes(bytes: u64) -> String {
    const MB: f64 = 1_000_000.0;
    if bytes >= MB as u64 {
        format!("{:.0} MB", bytes as f64 / MB)
    } else {
        format!("{bytes} B")
    }
}

#[derive(Parser)]
#[command(
    name = "tessera",
    about = "One search bar for everything your PC can't search",
    version
)]
struct Cli {
    /// Where the index lives. Defaults to the local app-data dir.
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,

    /// Where the models live. Defaults to a `models` folder beside the index.
    #[arg(long, global = true)]
    models_dir: Option<PathBuf>,

    /// Which OCR engine reads text out of images. Defaults to the platform's
    /// own, which is the fast path on every platform we ship.
    #[arg(long, global = true, value_name = "ENGINE")]
    ocr_engine: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Index every supported file under a folder (text, PDF, Word and images).
    Index { folder: PathBuf },
    /// Search the index: the keyword, image and meaning arms, fused into one
    /// ranking. Terms and filters in the same bar — `tessera search invoice
    /// type:pdf "in:My Documents" after:2024-01-01`.
    Search {
        /// Search terms and filters. Quote phrases as you would for a search
        /// engine: `tessera search "invoice airline"`. A token that is not a
        /// filter is a term (so `rfc:9110` searches for what was typed).
        ///
        /// Filters: `type:image`, `type:pdf`, `type:doc` (Word and text),
        /// `in:<folder>`, `before:<YYYY-MM-DD>`, `after:<YYYY-MM-DD>`.
        query: Vec<String>,
        /// Cut the query down to the image arm alone: what an image *looks
        /// like*, rather than the words on or in it. This is a diagnostic —
        /// the default search already runs this arm, weighted — and it is the
        /// only way to find a photo of a thing whose words you do not know.
        ///
        /// On screenshots the text arm wins by a wide margin (0.964 vs 0.268
        /// top-1 on 56 real files), so reach for this when keywords have
        /// nothing to go on.
        #[arg(long, value_name = "DESCRIPTION")]
        describe: Option<String>,
        /// Cut the query down to the meaning arm alone: MiniLM over every
        /// chunk the index holds. A diagnostic, like `--describe`: the default
        /// search fuses this arm with the others, and this flag is how the arm
        /// gets tuned and measured on its own.
        #[arg(long, value_name = "MEANING", conflicts_with = "describe")]
        describe_text: Option<String>,
        /// How many results to show. Defaults to 10.
        #[arg(long, short, default_value_t = 10)]
        limit: usize,
    },
    /// Read the text out of one image and print it, with timings. Exists so the
    /// two engines can be compared from a terminal on the same file, which is how
    /// the choice between them gets made.
    Ocr {
        /// The image to read.
        image: PathBuf,
        /// Which engine to read it with. Defaults to the best one available.
        #[arg(long, value_name = "ENGINE")]
        engine: Option<String>,
        /// Print each line with the confidence behind it, rather than just the
        /// text. This is what shows a score gate dropping a line.
        #[arg(long)]
        scores: bool,
    },
    /// Index counts by status.
    Stats,
    /// Keep indexed folders fresh while the machine is in use: a file saved now
    /// is searchable in seconds, at idle priority, and the queue pauses on
    /// battery by default. Runs until Ctrl-C.
    Watch {
        /// Folders to watch. Defaults to every folder the index already knows.
        folders: Vec<PathBuf>,
        /// When indexing may run: `idle-only` (the default) pauses the queue on
        /// battery and resumes on AC, `always` ignores the power state, and
        /// `paused` starts paused until you change it.
        #[arg(long, default_value_t = crate::watch::Schedule::default(), value_name = "POLICY")]
        schedule: crate::watch::Schedule,
    },
    /// Download and verify models. This is the only command that touches the
    /// network, and it never runs on its own.
    Models {
        #[command(subcommand)]
        action: ModelsAction,
    },
}

#[derive(Subcommand, Debug)]
enum ModelsAction {
    /// Fetch every pinned model. Downloads happen once; a second run verifies
    /// what is already there and touches no network.
    Install {
        /// Only these model sets, by name. Defaults to all of them.
        #[arg(long)]
        set: Vec<String>,
    },
    /// What is installed, what is missing, and which model version each arm's
    /// vectors came from.
    Status,
}

/// The default data dir: the OS local-data dir (e.g. %LOCALAPPDATA%\Tessera).
pub fn default_data_dir() -> Result<PathBuf> {
    let base = dirs::data_local_dir().context("the OS has no local data directory")?;
    Ok(base.join("Tessera"))
}

/// Every pipeline the engine has, in the order they are offered a file.
///
/// Public because it is the engine's format table, not a CLI detail: the
/// desktop shell runs the same pipelines through the same store, and a second
/// copy of this list would be a second answer to "which formats does Tessera
/// index".
///
/// Images need an OCR engine, which is chosen rather than constructed here:
/// the default is the platform's own engine, falling back to the cross-platform
/// one, and `--ocr-engine` overrides both. Constructing an engine that this
/// machine cannot run would make `tessera index` fail before it started, so the
/// choice is resolved inside the pipeline and a missing engine shows up per file,
/// in the diagnostics.
pub fn pipelines(ocr: &OcrChoice, models: &Models) -> Result<Vec<Box<dyn Pipeline>>> {
    // An engine the user *named* and did not get is a bug, not a preference, so
    // that stops the run. The platform default missing is not: the text formats
    // keep working and each image is recorded as failed with the reason.
    let engine: Box<dyn OcrEngine> = match ocr::select(ocr, models) {
        Ok(selection) => selection.engine,
        Err(err) if matches!(ocr, OcrChoice::Native) => {
            eprintln!("no OCR engine on this machine: {err:#}");
            Box::new(ocr::Unavailable(ocr::default_engine_name()))
        }
        Err(err) => return Err(err),
    };
    Ok(vec![
        Box::new(TextPipeline),
        Box::new(PdfPipeline),
        Box::new(DocxPipeline),
        Box::new(ImagePipeline::new(engine).with_embedder(image_embedder(models))),
    ])
}

/// Whether every file of a set is at least present on disk.
///
/// Used only on the failure path of an embedder constructor, to choose between
/// silence and a warning. Presence, not verification: re-hashing a gigabyte to
/// phrase a warning is how the warning becomes worse than the problem.
fn set_present(models: &Models, set: &str) -> bool {
    models
        .manifest()
        .set(set)
        .is_some_and(|set| set.files.iter().all(|file| models.path_of(file).exists()))
}

/// The image arm, or `None` when its models are not installed.
///
/// Optional on purpose. Loading a vision tower costs 213 MB and ~1 s of start-up,
/// and a user who has never asked for description search should not pay it, so
/// the arm is attached only when the pinned set is present and verified. Its
/// absence is announced once, because silently indexing without vectors looks
/// exactly like description search working and returning nothing.
///
/// Built first, asked about afterwards: constructing the embedder verifies the
/// set, so asking `is_installed` up front would hash the same files twice on
/// every load — and the siglip set is about a gigabyte. A set that is not
/// installed is silence; a set whose files are all there but which will not
/// load or does not match its pins is worth saying out loud.
pub fn image_embedder(models: &Models) -> Option<std::sync::Arc<SiglipEmbedder>> {
    image_embedder_at(models, default_threads())
}

/// The image arm for a query on the paired models: two threads, alongside the
/// text session rather than alone.
pub fn image_query_embedder(models: &Models) -> Option<std::sync::Arc<SiglipEmbedder>> {
    image_embedder_at(models, query_threads())
}

fn image_embedder_at(models: &Models, threads: usize) -> Option<std::sync::Arc<SiglipEmbedder>> {
    match SiglipEmbedder::new(models, threads) {
        Ok(embedder) => Some(std::sync::Arc::new(embedder)),
        Err(err) => {
            if set_present(models, siglip::MODEL_SET) {
                eprintln!(
                    "the siglip models are present but the embedder would not load them, so the \
                     image arm is off: {err:#}"
                );
            }
            None
        }
    }
}

/// The text arm's embedder, when its model set is installed and loads.
///
/// `None` is not an error: an index without MiniLM is still a keyword index,
/// and a folder of documents should not fail because an optional model is
/// missing. The caller says so once rather than once per file. Built first for
/// [`image_embedder`]'s reason: the constructor already verifies the set.
pub fn text_embedder(models: &Models) -> Option<MiniLm> {
    text_embedder_at(models, default_threads())
}

/// The meaning arm for a query on the paired models: two threads, alongside
/// the image session rather than alone.
pub fn text_query_embedder(models: &Models) -> Option<MiniLm> {
    text_embedder_at(models, query_threads())
}

fn text_embedder_at(models: &Models, threads: usize) -> Option<MiniLm> {
    match MiniLm::new(models, threads) {
        Ok(embedder) => Some(embedder),
        Err(err) => {
            if set_present(models, minilm::MODEL_SET) {
                eprintln!(
                    "the minilm models are present but the embedder would not load them, so the \
                     meaning arm is off: {err:#}"
                );
            }
            None
        }
    }
}

/// A CPU-bound session should not let ONNX Runtime spawn a thread per core and
/// fight the rest of the engine for them.
fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8)
}

/// Threads for the query-time ONNX sessions, not the indexing ones.
///
/// A fused query embeds on two sessions at once; handing both eight intra-op
/// threads doubles each embed's wall, because SigLIP's kernels occupy the
/// cores MiniLM needs. One sequence is far too little work to fill a machine,
/// so the cap is about contention rather than size. Measured on the 5,000-file
/// gate corpus, the fused query falls from 172 ms p50 (both sessions at four
/// threads) to 130 ms p50 with both at two; one thread is worse again because
/// each session alone is slower than the next pass is helped by the free cores.
fn query_threads() -> usize {
    2
}

/// Entry point for the binary; returns the process exit code.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let data_dir = match cli.data_dir {
        Some(dir) => dir,
        None => default_data_dir()?,
    };
    let store = Store::open(&data_dir)?;
    match cli.command {
        Command::Index { folder } => {
            // dunce keeps Windows paths user-friendly (no \\?\ verbatim prefix)
            let folder = dunce::canonicalize(&folder).context("resolving the folder")?;
            let ocr = cli
                .ocr_engine
                .as_deref()
                .map(|name| OcrChoice::Named(name.to_string()))
                .unwrap_or(OcrChoice::Native);
            let models = Models::open(&models_dir(&data_dir, cli.models_dir.as_deref()))?;
            if !ocr::is_available(&models) && ocr == OcrChoice::Native {
                // Said before the run, not after an hour of empty screenshots.
                eprintln!(
                    "warning: no OCR engine is available on this machine, so images will be \
                     recorded as failed. Text formats are unaffected.\n\
                     Install the cross-platform engine with: tessera models install --set {}",
                    crate::ocr::onnx::MODEL_SET
                );
            }
            let pipelines = pipelines(&ocr, &models)?;
            // A moved pin means the old vectors are from another space. Drop
            // the whole text arm before the sweep rather than mixing spaces;
            // the sweep rebuilds it from the tiles, so re-indexing any one
            // folder repairs the whole index. The image arm is left alone here
            // — it has no equivalent sweep, and dropping its vectors would make
            // them unrecoverable without walking every image again.
            for stale in models.stale_arms(&store)? {
                if stale.arm == Arm::Text && stale.is_from_a_different_model() {
                    store.forget_text_arm()?;
                    eprintln!(
                        "warning: the MiniLM pin moved (vectors built by {}, pinned is {}); \
                         dropping the text arm's vectors, which this run rebuilds from the text \
                         already indexed",
                        stale.built_by.as_deref().unwrap_or("an unknown model"),
                        stale.pinned
                    );
                }
            }
            let text = text_embedder(&models);
            let summary = match &text {
                Some(text) => index_folder_with_text(&store, &pipelines, &folder, text)?,
                None => index_folder(&store, &pipelines, &folder)?,
            };
            println!(
                "indexed {indexed}, {unchanged} unchanged, {moved} moved, {skipped} skipped, \
                 {failed} failed, {awaiting} awaiting OCR, {removed} removed",
                indexed = summary.indexed,
                unchanged = summary.unchanged,
                moved = summary.moved,
                skipped = summary.skipped,
                failed = summary.failed,
                awaiting = summary.awaiting_ocr,
                removed = summary.removed,
            );
            if text.is_some() {
                println!(
                    "{} text chunk(s) embedded for meaning search",
                    summary.text_embedded
                );
            }
            if summary.text_unembedded > 0 {
                println!(
                    "{} text chunk(s) could not be embedded; they stay searchable by keyword",
                    summary.text_unembedded
                );
            }
            if summary.walk_errors > 0 {
                println!("{} unreadable path(s)", summary.walk_errors);
            }
            println!("index at {}", data_dir.join("index.sqlite").display());
            Ok(())
        }
        Command::Ocr {
            image,
            engine,
            scores,
        } => {
            let choice = engine
                .as_deref()
                .map(|name| OcrChoice::Named(name.to_string()))
                .unwrap_or(OcrChoice::Native);
            let models = Models::open(&models_dir(&data_dir, cli.models_dir.as_deref()))?;
            let selection = ocr::select(&choice, &models)?;
            let image = dunce::canonicalize(&image).context("resolving the image")?;
            let started = std::time::Instant::now();
            let lines = selection.engine.lines_with_scores(&image)?;
            let elapsed = started.elapsed();
            for (line, score) in &lines {
                if scores {
                    match score {
                        Some(score) => println!("{score:>5.2}  {line}"),
                        None => println!("{:-^5}  {line}", "n/a"),
                    }
                } else {
                    println!("{line}");
                }
            }
            println!(
                "{}: {} line(s), {} character(s), {:.1} ms",
                selection.name,
                lines.len(),
                lines
                    .iter()
                    .map(|(text, _)| text.chars().count())
                    .sum::<usize>(),
                elapsed.as_secs_f64() * 1000.0
            );
            Ok(())
        }
        Command::Search {
            query,
            describe,
            describe_text,
            limit,
        } => {
            // `--describe` is the other arm, not a rephrasing of this one: it
            // embeds the words and compares them with what each image looks
            // like. The two are not merged, because on files that contain text
            // the keyword arm is much the stronger of the two.
            if let Some(description) = describe {
                let models = Models::open(&models_dir(&data_dir, cli.models_dir.as_deref()))?;
                let Some(embedder) = image_query_embedder(&models) else {
                    anyhow::bail!(
                        "description search needs the siglip models, which are not installed. \
                         Fix it with: tessera models install --set siglip"
                    );
                };
                let embedder = std::sync::Arc::new(embedder);
                let started = std::time::Instant::now();
                let vector = embedder.embed_query(&description, IMAGE_EMBEDDING_DIM)?;
                let hits = store.describe(&vector, IMAGE_EMBEDDING_DIM, limit)?;
                eprintln!(
                    "described in {:.0} ms",
                    started.elapsed().as_secs_f64() * 1000.0
                );
                if hits.is_empty() {
                    println!("no results");
                    println!(
                        "  (the image arm searched {} embedded image(s); if that is 0, run \
                         `tessera index <folder>` again now that the models are installed)",
                        store.image_embedding_count()?
                    );
                    return Ok(());
                }
                for hit in hits {
                    println!("{}", hit.path);
                    if let Some(tile) = hit.tiles.first() {
                        println!("  {}", tile.text.chars().take(120).collect::<String>());
                    }
                }
                return Ok(());
            }

            if let Some(meaning) = describe_text {
                let models = Models::open(&models_dir(&data_dir, cli.models_dir.as_deref()))?;
                let Some(embedder) = text_query_embedder(&models) else {
                    anyhow::bail!(
                        "meaning search needs the minilm models, which are not installed. \
                         Fix it with: tessera models install --set minilm"
                    );
                };
                let started = std::time::Instant::now();
                let vector = embedder.embed_query(&meaning)?;
                let hits = store.describe_text(&vector, TEXT_EMBEDDING_DIM, limit)?;
                eprintln!(
                    "searched meaning in {:.0} ms",
                    started.elapsed().as_secs_f64() * 1000.0
                );
                if hits.is_empty() {
                    println!("no results");
                    println!(
                        "  (the text arm searched {} embedded chunk(s); if that is 0, run \
                         `tessera index <folder>` again now that the models are installed)",
                        store.text_embedding_count()?
                    );
                    return Ok(());
                }
                for hit in hits {
                    println!("{}", hit.path);
                    if let Some(tile) = hit.tiles.first() {
                        // A window, collapsed to one line: the tile starts with
                        // the file's name (index::with_filename), and printing
                        // that again under the path it just printed reads as a
                        // bug even when it is the text on file.
                        let preview = snippet_around(&tile.text, &[], 160);
                        match tile.page {
                            Some(page) => println!("  page {page}: {preview}"),
                            None => println!("  {preview}"),
                        }
                    }
                }
                return Ok(());
            }
            // The default: the one search box. Every arm this machine has runs
            // over the query, and the fusion weighs their votes by what the
            // index knows about each file (docs/adr/0003). The two flags above
            // stay as diagnostics — they cut the query down to a single arm.
            let parsed = parse_query(&query)?;
            if parsed.terms.is_empty() && parsed.filter.is_empty() {
                anyhow::bail!("nothing to search for — give a word, or a filter like type:pdf");
            }
            let models = Models::open(&models_dir(&data_dir, cli.models_dir.as_deref()))?;
            let image = image_query_embedder(&models);
            let text = text_query_embedder(&models);
            // Measured after the embedders are built: this number is the
            // search, not the model loading a process pays once.
            let started = std::time::Instant::now();
            let outcome = fused_search(
                &data_dir,
                &parsed,
                image
                    .as_deref()
                    .map(|embedder| embedder as &dyn QueryEmbedder),
                text.as_ref().map(|embedder| embedder as &dyn TextEmbedder),
                limit,
            )?;
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            if outcome.hits.is_empty() {
                println!("no results");
            }
            for hit in &outcome.hits {
                let arms = if hit.arms.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", hit.arms.names().join(", "))
                };
                println!("{}{arms}", hit.path);
                match hit.page {
                    Some(page) => println!("  page {page}: {}", hit.snippet),
                    None => println!("  {}", hit.snippet),
                }
            }
            for note in &outcome.notes {
                eprintln!("note: {note}");
            }
            // The timing line goes to the terminal, not the pipe: what stdout
            // carries is results.
            let arms: Vec<String> = outcome
                .timings
                .iter()
                .map(|timing| format!("{} {:.0} ms", timing.arm.name(), timing.millis))
                .collect();
            eprintln!("searched in {elapsed:.0} ms ({})", arms.join(", "));
            Ok(())
        }
        Command::Stats => {
            for line in stats_report(&store)? {
                println!("{line}");
            }
            Ok(())
        }
        Command::Watch { folders, schedule } => {
            // The first Ctrl-C asks the loop to stop after the file in flight;
            // a second one is the user saying they mean it.
            let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = shutdown.clone();
            ctrlc::set_handler(move || {
                if flag.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    eprintln!("second interrupt: stopping now");
                    std::process::exit(130);
                }
                eprintln!("stopping after the current file (Ctrl-C again to stop immediately)");
            })
            .context("installing the Ctrl-C handler")?;

            // No folders means "the corpus you already have": watching what the
            // store owns is the natural meaning of a background indexer with no
            // arguments, and it cannot watch a folder nobody indexed.
            let roots = if folders.is_empty() {
                store.roots()?
            } else {
                folders
                    .iter()
                    .map(|folder| dunce::canonicalize(folder).context("resolving the folder"))
                    .collect::<Result<Vec<_>>>()?
            };
            if roots.is_empty() {
                anyhow::bail!(
                    "nothing to watch: index a folder first, or name one: tessera watch <folder>"
                );
            }
            let models = Models::open(&models_dir(&data_dir, cli.models_dir.as_deref()))?;
            let ocr = cli
                .ocr_engine
                .as_deref()
                .map(|name| OcrChoice::Named(name.to_string()))
                .unwrap_or(OcrChoice::Native);
            let options = crate::watch::Options {
                roots,
                schedule,
                ..Default::default()
            };
            let summary = crate::watch::run(&data_dir, models, ocr, options, &shutdown)?;
            println!(
                "stopped: {indexed} indexed, {unchanged} unchanged, {moved} moved, \
                 {skipped} skipped, {failed} failed, {awaiting} awaiting OCR, {removed} removed, \
                 {embedded} chunk(s) embedded",
                indexed = summary.indexed,
                unchanged = summary.unchanged,
                moved = summary.moved,
                skipped = summary.skipped,
                failed = summary.failed,
                awaiting = summary.awaiting_ocr,
                removed = summary.removed,
                embedded = summary.text_embedded,
            );
            // The counters that mean "this run did not do what you think it
            // did" are always printed, zeros included: a watcher that could not
            // record files, could not walk folders, or could not update the
            // index must not end on a line that reads like a clean run.
            println!(
                "         {walk_errors} walk error(s), {errors} file(s) could not be recorded, \
                 {coordinator} index update(s) failed, {deferred} waiting to retry",
                walk_errors = summary.walk_errors,
                errors = summary.errors,
                coordinator = summary.coordinator_errors,
                deferred = summary.deferred,
            );
            Ok(())
        }
        Command::Models { action } => {
            let dir = models_dir(&data_dir, cli.models_dir.as_deref());
            let models = Models::open(&dir)?;
            match action {
                ModelsAction::Install { set } => {
                    let sets: Vec<&str> = if set.is_empty() {
                        models.set_names()
                    } else {
                        set.iter().map(String::as_str).collect()
                    };
                    let report = models.ensure(&sets)?;
                    for line in report.lines() {
                        println!("{line}");
                    }
                    // Silence would be a lie here: "nothing to do" and "nothing
                    // happened because nothing was asked for" look identical.
                    println!(
                        "{} file(s) downloaded, {} already verified, {} in {}",
                        report.downloaded.len(),
                        report.already_present.len(),
                        human_bytes(report.bytes),
                        models.dir().display()
                    );
                    Ok(())
                }
                ModelsAction::Status => {
                    println!("models in {}", models.dir().display());
                    for set in models.status()? {
                        // A set with no arm has no vector space at all — OCR
                        // models produce text — so it gets no version report.
                        let arm = match set.arm {
                            Some(arm) => format!(" [{arm:?} arm]"),
                            None => String::new(),
                        };
                        println!("\n{}{arm}", set.name);
                        for file in &set.installed {
                            println!("  ok      {file}");
                        }
                        for file in &set.missing {
                            println!("  missing {file}");
                        }
                        if let Some(arm) = set.arm {
                            match store.arm_version(arm)? {
                                Some(recorded) if recorded == set.version => {
                                    println!("  vectors match this model version");
                                }
                                Some(recorded) => println!(
                                    "  VECTORS STALE: built by {recorded}, pinned is {}",
                                    set.version
                                ),
                                None => println!("  no vectors yet for this arm"),
                            }
                        }
                    }
                    Ok(())
                }
            }
        }
    }
}

/// The lines `tessera stats` prints: file counts per status, then a breakdown
/// of *why* files were skipped.
///
/// "skipped 12" answers nothing on its own; "4 of them were cloud files we
/// refused to download" answers the question people actually have, which is
/// whether something they know exists is missing from their search.
///
/// Returned as lines rather than printed from here so the report is a value a
/// test can hold — the diagnostics are an acceptance criterion, not a side
/// effect.
pub fn stats_report(store: &Store) -> Result<Vec<String>> {
    let statuses = store.stats()?;
    if statuses.is_empty() {
        return Ok(vec!["index is empty".to_string()]);
    }
    let mut lines: Vec<String> = statuses
        .iter()
        .map(|(status, count)| format!("{status:<12} {count}"))
        .collect();
    lines.extend(
        store
            .skip_reasons()?
            .iter()
            .map(|(reason, count)| format!("  {reason:<18} {count}")),
    );
    // A count of failures answers nothing without the file names, and "search
    // can't find my scanned invoice" is a question about a file.
    for (path, reason) in store.failures()? {
        lines.push(format!("  FAILED {path}"));
        lines.push(format!("         {reason}"));
    }
    Ok(lines)
}

/// Index a folder with the engine's current pipelines. Used by the CLI and by
/// tests, so both exercise the same path.
///
/// `models` is passed in rather than opened here so the caller owns where models
/// are read from; tests point it at an empty directory, which is exactly the
/// "no OCR engine installed" state a fresh machine is in.
pub fn index_text_folder(
    store: &Store,
    models: &Models,
    folder: &Path,
) -> Result<crate::index::IndexSummary> {
    index_folder(store, &pipelines(&OcrChoice::Native, models)?, folder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::skip::{FileFacts, SkipReason};
    use crate::index::{Kind, Tile};

    fn facts(size: u64) -> FileFacts {
        FileFacts {
            size,
            mtime: 0,
            placeholder: false,
        }
    }

    #[test]
    fn the_models_dir_flag_overrides_the_default_beside_the_index() {
        // The override exists so tests — and people who keep models on another
        // disk — can point somewhere else without moving anything.
        assert_eq!(
            models_dir(Path::new("/data"), None),
            PathBuf::from("/data").join("models")
        );
        assert_eq!(
            models_dir(Path::new("/data"), Some(Path::new("/elsewhere/models"))),
            PathBuf::from("/elsewhere/models")
        );
    }

    #[test]
    fn the_models_commands_parse() {
        use clap::Parser;
        let cli = Cli::try_parse_from([
            "tessera",
            "--data-dir",
            "/data",
            "--models-dir",
            "/models",
            "models",
            "install",
            "--set",
            "ppocr",
        ])
        .expect("the models command parses");
        assert_eq!(cli.data_dir, Some(PathBuf::from("/data")));
        assert_eq!(cli.models_dir, Some(PathBuf::from("/models")));
        match cli.command {
            Command::Models {
                action: ModelsAction::Install { set },
            } => assert_eq!(set, vec!["ppocr".to_string()]),
            other => panic!("expected an install command, got {other:?}"),
        }

        let status = Cli::try_parse_from(["tessera", "models", "status"]).expect("status parses");
        assert!(matches!(
            status.command,
            Command::Models {
                action: ModelsAction::Status
            }
        ));
    }

    /// An empty models directory: what a machine that has never installed the
    /// OCR models looks like, and what these tests want so that engine choice
    /// does not depend on the developer's machine.
    fn empty_models() -> Models {
        let dir =
            std::env::temp_dir().join(format!("tessera-cli-no-models-{}", std::process::id()));
        Models::open(&dir).expect("an empty models dir is always openable")
    }

    #[test]
    fn the_ocr_engine_flag_is_the_config_override() {
        use clap::Parser;
        // Default: the platform's own engine.
        let default = Cli::try_parse_from(["tessera", "index", "C:/corpus"]).unwrap();
        assert_eq!(default.ocr_engine, None, "no flag means the native engine");

        // Override: an engine the user named.
        let named =
            Cli::try_parse_from(["tessera", "--ocr-engine", "onnx", "index", "C:/corpus"]).unwrap();
        assert_eq!(named.ocr_engine.as_deref(), Some("onnx"));

        // And an engine that does not exist stops the run, rather than being
        // silently replaced by something else.
        let Err(err) = pipelines(&OcrChoice::Named("tesseract".into()), &empty_models()) else {
            panic!("a named engine we do not have must not resolve");
        };
        assert!(format!("{err:#}").contains("tesseract"));

        // The native default still resolves when the machine has no engine: that
        // is per-file diagnostics, not a refusal to index.
        assert!(pipelines(&OcrChoice::Native, &empty_models()).is_ok());
    }

    #[test]
    fn human_bytes_reads_the_way_a_setup_screen_needs() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(23_000_000), "23 MB");
    }

    #[test]
    fn the_stats_report_says_why_files_were_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(stats_report(&store).unwrap(), vec!["index is empty"]);

        let corpus = tempfile::tempdir().unwrap();
        std::fs::write(corpus.path().join("ok.txt"), "the invoice").unwrap();
        store
            .index_file(
                corpus.path(),
                &corpus.path().join("ok.txt"),
                Kind::Text,
                &facts(10),
                Some("abc"),
                &[Tile {
                    seq: 0,
                    text: "ok.txt\nthe invoice".into(),
                    page: None,
                }],
            )
            .unwrap();
        store
            .skip_file(
                corpus.path(),
                &corpus.path().join("cloud.png"),
                Kind::Image,
                &FileFacts {
                    placeholder: true,
                    ..facts(0)
                },
                SkipReason::CloudPlaceholder,
            )
            .unwrap();

        assert_eq!(
            stats_report(&store).unwrap(),
            vec![
                "indexed      1".to_string(),
                "skipped      1".to_string(),
                "  cloud_placeholder  1".to_string(),
            ]
        );
    }

    #[test]
    fn the_stats_report_names_every_file_that_failed_and_why() {
        // The AC says stats prints indexed / skipped / failed. The other test in
        // this file covered the first two and a placeholder, and this branch —
        // the one a user actually runs `stats` for after "search can't find my
        // scanned invoice" — was never exercised. Deleting `cli.rs:339-342`
        // would have broken nothing before this test.
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let broken = corpus.path().join("scan.png");
        std::fs::write(&broken, b"not really an image").unwrap();

        store
            .index_file(
                corpus.path(),
                &corpus.path().join("ok.txt"),
                Kind::Text,
                &facts(10),
                Some("abc"),
                &[Tile {
                    seq: 0,
                    text: "ok.txt\nthe invoice".into(),
                    page: None,
                }],
            )
            .unwrap();
        store
            .fail_file(
                corpus.path(),
                &broken,
                Kind::Image,
                &facts(64),
                "the bitmap header is not a bitmap",
            )
            .unwrap();

        let report = stats_report(&store).unwrap();
        assert!(report.contains(&"indexed      1".to_string()), "{report:?}");
        assert!(report.contains(&"failed       1".to_string()), "{report:?}");
        // The count alone answers nothing: the file name and the reason are the
        // whole point of asking.
        assert!(
            report
                .iter()
                .any(|line| line.contains("FAILED") && line.contains("scan.png")),
            "the report does not name the failed file: {report:?}"
        );
        assert!(
            report
                .iter()
                .any(|line| line.contains("the bitmap header is not a bitmap")),
            "the report does not say why it failed: {report:?}"
        );
    }
}
