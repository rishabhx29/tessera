//! Tessera core: the indexing and search engine.
//!
//! Status: the week 2–3 engine is being built ticket by ticket from
//! `docs/product-definition.md`; this crate currently contains the search
//! engine end to end: the SQLite store (full-text and vectors), the text, PDF,
//! DOCX and image pipelines, both OCR engines behind one seam, lexical search,
//! the SigLIP image arm and the MiniLM text arm, incremental indexing (change
//! detection, skip rules, move and deletion propagation), and the CLI.
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
pub mod vectors;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
