//! The `tessera` command-line interface: index, search, stats.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::index::{index_folder, text::TextPipeline, Pipeline};
use crate::models::Models;
use crate::search::search;
use crate::store::Store;

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
fn pipelines() -> Vec<Box<dyn Pipeline>> {
    vec![Box::new(TextPipeline)]
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
            let summary = index_folder(&store, &pipelines(), &folder)?;
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
        Command::Search { query } => {
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
    Ok(lines)
}

/// Index a folder with the engine's current pipelines. Used by the CLI and by
/// tests, so both exercise the same path.
pub fn index_text_folder(store: &Store, folder: &Path) -> Result<crate::index::IndexSummary> {
    index_folder(store, &pipelines(), folder)
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
}
