//! The `tessera` command-line interface: index, search, stats.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::index::{index_folder, text::TextPipeline, Pipeline};
use crate::search::search;
use crate::store::Store;

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

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
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
