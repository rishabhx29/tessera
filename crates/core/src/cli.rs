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
        /// Search terms, e.g. `tessera search invoice airline`
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
            let pipelines: Vec<Box<dyn Pipeline>> = vec![Box::new(TextPipeline)];
            let (indexed, failed) = index_folder(&store, &pipelines, &folder)?;
            println!(
                "indexed {indexed} file(s), {failed} failed, index at {}",
                data_dir.join("index.sqlite").display()
            );
            Ok(())
        }
        Command::Search { query } => {
            let hits = search(&store, &query)?;
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

/// Open a store over `data_dir` and index `folder` with the text pipeline.
/// Used by tests so the CLI's behavior is exercised without spawning a binary.
pub fn index_text_folder(store: &Store, folder: &Path) -> Result<(usize, usize)> {
    let pipelines: Vec<Box<dyn Pipeline>> = vec![Box::new(TextPipeline)];
    index_folder(store, &pipelines, folder)
}
