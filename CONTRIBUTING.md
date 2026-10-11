# Contributing to Tessera

Thanks for considering a contribution. The project is pre-alpha, so the most
useful help right now is design discussion, testing plans, and careful reports —
not large code dumps.

## Ground rules

- Read [`docs/product-definition.md`](docs/product-definition.md) first. It is
  the locked spec; changes that contradict it start as discussion, not PRs.
- Non-trivial or hard-to-reverse decisions get an ADR before the code
  ([`docs/adr/`](docs/adr/)).
- Every PR must be green on: `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --all`.
- No performance numbers in code comments, docs, or PR descriptions unless they
  were measured, with hardware and methodology attached
  ([`docs/performance.md`](docs/performance.md)).
- Don't paste private content (screenshots of chats, passwords, bank details)
  into issues — OCR extracts text from images, so the index and your reports
  can leak it.

## Development setup

See [`docs/architecture.md`](docs/architecture.md) for the crate layout. The
engine is `crates/core`; the desktop shell is `crates/app` (Tauri, from week 4).

## Reporting bugs

Use the bug report template. Include your Windows version, an approximate count
of indexed files, and the languages involved — all three change behavior.

## Code of Conduct

Participation is governed by the
[Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md).
