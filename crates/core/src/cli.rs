//! The `tessera` command-line interface: index, search, stats.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::index::{image::ImagePipeline, index_folder, text::TextPipeline, Pipeline};
use crate::models::Models;
use crate::ocr::{self, OcrChoice, OcrEngine};
use crate::search::search;
use crate::store::{Store, IMAGE_EMBEDDING_DIM};
use crate::vectors::siglip::SiglipEmbedder;

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
    /// Index every supported file under a folder (text formats today).
    Index { folder: PathBuf },
    /// Search the index. Multiple terms all must appear in the same file.
    Search {
        /// Search terms. Quote them as you would for a search engine:
        /// `tessera search "invoice airline"`.
        query: Vec<String>,
        /// Describe what you are looking for instead of naming words in it:
        /// `tessera search --describe "a chart about flight delays"`.
        ///
        /// This searches by what an image *looks like*, which is the only way to
        /// find a photo of a thing whose words you do not know. It is a
        /// different arm from keyword search, not a better one — on screenshots
        /// the text arm wins by a wide margin (0.964 vs 0.268 top-1 on 56 real
        /// files), so reach for this when keywords have nothing to go on.
        #[arg(long, value_name = "DESCRIPTION")]
        describe: Option<String>,
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

/// Split raw CLI arguments into search terms. Each argument is tokenized, so
/// one quoted `"invoice airline"` becomes the two terms the user meant rather
/// than one phrase that matches nothing.
pub fn split_terms(args: &[String]) -> Vec<String> {
    args.iter()
        .flat_map(|a| a.split_whitespace())
        .map(str::to_string)
        .collect()
}

/// Every pipeline the engine has, in the order they are offered a file.
///
/// Images need an OCR engine, which is chosen rather than constructed here:
/// the default is the platform's own engine, falling back to the cross-platform
/// one, and `--ocr-engine` overrides both. Constructing an engine that this
/// machine cannot run would make `tessera index` fail before it started, so the
/// choice is resolved inside the pipeline and a missing engine shows up per file,
/// in the diagnostics.
fn pipelines(ocr: &OcrChoice, models: &Models) -> Result<Vec<Box<dyn Pipeline>>> {
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
        Box::new(ImagePipeline::new(engine).with_embedder(image_embedder(models))),
    ])
}

/// The image arm, or `None` when its models are not installed.
///
/// Optional on purpose. Loading a vision tower costs 213 MB and ~1 s of start-up,
/// and a user who has never asked for description search should not pay it, so
/// the arm is attached only when the pinned set is present and verified. Its
/// absence is announced once, because silently indexing without vectors looks
/// exactly like description search working and returning nothing.
pub fn image_embedder(models: &Models) -> Option<std::sync::Arc<SiglipEmbedder>> {
    if !SiglipEmbedder::is_installed(models) {
        return None;
    }
    match SiglipEmbedder::new(models, default_threads()) {
        Ok(embedder) => Some(std::sync::Arc::new(embedder)),
        Err(err) => {
            eprintln!(
                "the siglip models are installed but the embedder would not load, so images will \
                 be indexed without description search: {err:#}"
            );
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
            let summary = index_folder(&store, &pipelines, &folder)?;
            println!(
                "indexed {indexed}, {unchanged} unchanged, {moved} moved, {skipped} skipped, \
                 {failed} failed, {removed} removed",
                indexed = summary.indexed,
                unchanged = summary.unchanged,
                moved = summary.moved,
                skipped = summary.skipped,
                failed = summary.failed,
                removed = summary.removed,
            );
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
            limit,
        } => {
            // `--describe` is the other arm, not a rephrasing of this one: it
            // embeds the words and compares them with what each image looks
            // like. The two are not merged, because on files that contain text
            // the keyword arm is much the stronger of the two.
            if let Some(description) = describe {
                let models = Models::open(&models_dir(&data_dir, cli.models_dir.as_deref()))?;
                let Some(embedder) = image_embedder(&models) else {
                    anyhow::bail!(
                        "description search needs the siglip models, which are not installed. \
                         Fix it with: tessera models install --set siglip"
                    );
                };
                let embedder = std::sync::Arc::new(embedder);
                let started = std::time::Instant::now();
                let vector = embedder.embed_text(&description, IMAGE_EMBEDDING_DIM)?;
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
                    if let Some(text) = hit.tiles.first() {
                        println!("  {}", text.chars().take(120).collect::<String>());
                    }
                }
                return Ok(());
            }

            let terms = split_terms(&query);
            let hits = search(
                &store,
                &terms.iter().map(String::as_str).collect::<Vec<_>>(),
            )?;
            if hits.is_empty() {
                println!("no results");
                return Ok(());
            }
            for hit in hits {
                println!("{}", hit.path);
                println!("  {}", hit.snippet);
            }
            Ok(())
        }
        Command::Stats => {
            for line in stats_report(&store)? {
                println!("{line}");
            }
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
        .map(|(status, count)| format!("{status:<10} {count}"))
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
    fn a_quoted_multiword_argument_becomes_two_terms() {
        let args = vec!["invoice airline".to_string()];
        assert_eq!(split_terms(&args), vec!["invoice", "airline"]);
    }

    #[test]
    fn arguments_and_whitespace_both_split() {
        let args = vec!["invoice".to_string(), "  from\tthe\tairline ".to_string()];
        assert_eq!(
            split_terms(&args),
            vec!["invoice", "from", "the", "airline"]
        );
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
                "indexed    1".to_string(),
                "skipped    1".to_string(),
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
        assert!(report.contains(&"indexed    1".to_string()), "{report:?}");
        assert!(report.contains(&"failed     1".to_string()), "{report:?}");
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
