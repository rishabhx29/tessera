//! Tessera core: the indexing and search engine.
//!
//! Status: the week 2–3 engine is being built ticket by ticket from
//! `docs/product-definition.md`; this crate currently contains the
//! text slice end to end: store, text pipeline, lexical search, incremental
//! indexing (change detection, skip rules, move and deletion propagation), and
//! the CLI.
//!
//! Everything the engine needs has a home in this crate: the folder watcher,
//! the per-format indexing pipelines, the SQLite store (full-text + vectors),
//! and hybrid search. The desktop shell (`tessera-app`) calls into this crate;
//! this crate never depends on the shell.

pub mod cli;
pub mod hash;
pub mod index;
pub mod models;
pub mod ocr;
pub mod search;
pub mod store;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
