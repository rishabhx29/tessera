//! Tessera core: the indexing and search engine.
//!
//! Status: scaffold only. The engine is built from week 2 of the plan in
//! `docs/product-definition.md`, and only after the week-1 spike has produced
//! real latency numbers for `docs/performance.md`.
//!
//! Everything the engine needs has a home in this crate: the folder watcher,
//! the per-format indexing pipelines, the SQLite store (full-text + vectors),
//! and hybrid search. The desktop shell (`tessera-app`) calls into this crate;
//! this crate never depends on the shell.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
