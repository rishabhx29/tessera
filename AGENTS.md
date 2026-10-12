# Tessera — agent guide

Instructions for AI coding agents working in this repository. Human
contributors: see [CONTRIBUTING.md](CONTRIBUTING.md).

## Commands

| Command | Purpose |
|---|---|
| `cargo fmt --all -- --check` | formatting gate (must pass) |
| `cargo clippy --all-targets -- -D warnings` | lint gate (must pass) |
| `cargo test --all` | test gate (must pass) |
| `cargo run -p tessera-core` | engine entry point (CLI, from week 2-3) |
| `cargo run -p tessera-app` | desktop shell (from week 4) |

## Doc map — read before changing behavior

- [`docs/product-definition.md`](docs/product-definition.md) — the locked spec.
  A change that contradicts it is wrong until an ADR says otherwise.
- [`docs/architecture.md`](docs/architecture.md) — crates, data model,
  per-format pipelines.
- [`docs/adr/`](docs/adr/) — decisions that are expensive to reverse.
- [`docs/performance.md`](docs/performance.md) — measured numbers only. Never
  write a target as though it were a result.
- [`CONTEXT.md`](CONTEXT.md) — shared vocabulary. Use these terms in code,
  docs, and commit messages so they stay unambiguous.

## Conventions

- Rust 2021 edition; rustfmt and clippy clean with `-D warnings`.
- `tessera-core` (engine) never depends on UI crates; `tessera-app` calls into it.
- Each file format is a pipeline behind one indexing trait, so a new format
  (video in v0.2) never touches the store or the search code.
- Model versions are pinned in the store's `meta` table. Changing a model
  invalidates vectors and forces re-indexing — never mix vector spaces.
- The index is user data. Anything that writes outside the designated index
  and cache directories is a bug by definition (see SECURITY.md).
- No telemetry, no network calls at query time, no accounts — ever. The only
  network access is the first-run model download, checksum-verified.

## Phase discipline

The build plan in `docs/product-definition.md` is ordered: week-1 spike, then
engine, then UI, then packaging, then launch. Work the current phase; don't
pull later-phase features forward because they're interesting.

## Agent skills

### Issue tracker

Local markdown files under `.scratch/<feature-slug>/` — spec at `spec.md`, one file per ticket under `issues/`. No hosted tracker. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default roles: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: root `CONTEXT.md` + `docs/adr/`. See `docs/agents/domain.md`.
