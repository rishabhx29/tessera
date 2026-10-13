# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/) once
releases begin.

## [Unreleased]

### Added

- Repository scaffold: MIT license, Contributor Covenant, security policy,
  issue templates, PR template, CI (fmt / clippy / test).
- Product Definition v1.0, architecture notes, and five ADRs (stack, license,
  search design, model choices, rollout plan and kill criteria).
- Rust workspace skeleton (`tessera-core`, `tessera-app`) with green CI.
- The engine's first path: SQLite store (WAL, FTS5 trigram over an
  external-content tiles table), text pipeline, typo-tolerant lexical search,
  and the `tessera index` / `search` / `stats` CLI.
- Incremental indexing: change detection by size + mtime, falling back to a
  streamed sha1 only when the cheap evidence disagrees. Re-indexing an
  unchanged folder reads nothing.
- Move and rename detection: a new path whose content hash matches a row under
  the same folder moves that row instead of extracting the same bytes twice,
  and re-titles the searchable text to the file's new name.
- Deletion propagation: rows whose file is gone from disk are dropped, scoped
  to the folder that was indexed.
- Skip rules recorded with reasons — cloud placeholders (Windows Files
  On-Demand attributes, never hydrated) and files above the 50 MB cap — plus a
  skip-reason breakdown in `tessera stats`.
- Store schema v2 (corpus `root` column, `root`+`sha1` index) with a tested
  v1 → v2 migration.

### Changed

- `tessera index` reports indexed / unchanged / moved / skipped / failed /
  removed counts.
