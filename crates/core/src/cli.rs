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
                "indexed {} file(s), {} failed, {} unreadable folder(s), index at {}",
                summary.indexed,
                summary.failed,
                summary.walk_errors,
                data_dir.join("index.sqlite").display()
            );
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
            let rows = store.stats()?;
            if rows.is_empty() {
                println!("index is empty");
            }
            for (status, count) in rows {
                println!("{status:<10} {count}");
            }
            Ok(())
        }
    }
}

/// Index a folder with the engine's current pipelines. Used by the CLI and by
/// tests, so both exercise the same path.
pub fn index_text_folder(store: &Store, folder: &Path) -> Result<crate::index::IndexSummary> {
    index_folder(store, &pipelines(), folder)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
