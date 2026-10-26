//! The desktop shell, minus the window.
//!
//! `engine` is everything the window asks the engine for: search, preview,
//! status, folder indexing, model state and rebuild. It has no windowing
//! dependency, so its tests run on a machine with no display and the shell
//! layer above it stays thin enough to read.
//!
//! The Tauri command layer lives in `main.rs`: one handler per method here,
//! passing JSON through. Nothing in this crate reaches around `tessera-core`
//! to the database.

pub mod engine;
pub mod watcher;

pub use engine::AppService;
pub use watcher::WatchHandle;
