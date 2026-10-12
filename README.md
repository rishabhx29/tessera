# Tessera

**One search bar for everything your PC can't search — images, screenshots, PDFs, and documents — offline, free, and open source.**

A *tessera* is one tile in a mosaic: every screenshot, image, page, and document is a fragment, and Tessera is the picture they add up to.

> **Status: pre-alpha.** The product definition and repository scaffold land first; the first release has not shipped. No performance claims are made anywhere in this repo until they are measured on real hardware — see [`docs/performance.md`](docs/performance.md).

## The problem

Windows treats images as pictures and a thousands-of-files folder as a maze:

- You can't search a screenshot by the words inside it.
- You can't search a saved image by what it shows.
- Filenames like `Screenshot 2026-10-11 at 14.32.11.png` describe nothing.
- Microsoft's semantic search answers this — but only on Copilot+ PCs, which most people don't own.

## What Tessera will do

Type **"the invoice from the airline"** into one search box and get back the screenshot, the PDF page, and the docx — anything your PC can't search today, from every folder you choose:

- **Search by the words inside an image** — OCR runs on every screenshot and saved image.
- **Search by describing what an image shows** — a local vision model, no cloud.
- **Search inside documents** — PDF, DOCX, and text files by their actual content, not their filenames.
- **Fast** — hybrid keyword + semantic search over tens of thousands of files, designed to work on older hardware without a GPU.
- **Private** — everything runs on your machine. No account, no telemetry, no uploads.
- **Open** — MIT-licensed. The same index becomes queryable by AI agents via MCP (v0.3).

## What Tessera is not

Not a screen recorder. Not a photo manager. Not cloud software. Not paid.

## Roadmap

- [x] Research, product definition, architecture ([`docs/`](docs/))
- [x] **Week 1 — spike:** measure OCR / vision-embedding latency on real hardware → measured, targets met after model swap ([`docs/performance.md`](docs/performance.md), [ADR-0006](docs/adr/0006-week-1-spike-model-decisions.md))
- [ ] **v0.1 — images + documents:** one search box, one index, Windows-first
- [ ] **v0.2 — video:** search inside recorded meetings, calls, and screen recordings
- [ ] **v0.3 — phone camera roll + MCP:** phone screenshots searchable on your PC; agents can query the index
- [ ] **v0.4 — auto-organize:** suggested names and filing, dry-run and opt-in only

## Rules this project holds itself to

1. Benchmarks are published only after they are measured, with hardware and methodology attached.
2. A roadmap item is checked off only when a release contains it.
3. If the launch doesn't find users, the project stops instead of rotting — the criteria are pre-committed in [ADR-0005](docs/adr/0005-phased-rollout-and-kill-criteria.md).

## Documentation

- [Product Definition v1.0](docs/product-definition.md) — the spec this repo builds from
- [Architecture](docs/architecture.md) — crates, data model, pipelines
- [ADRs](docs/adr/) — the decisions that are expensive to reverse
- [Contributing](CONTRIBUTING.md) · [Code of Conduct](CODE_OF_CONDUCT.md) · [Security](SECURITY.md)

## License

[MIT](LICENSE)
