# Tessera — project context

Shared vocabulary and current state. Update this file when terminology or the
phase changes; superseded decisions get an ADR, not an edit in place.

## What Tessera is

One offline search bar for everything a PC can't natively search: images (by the
words inside them and by description), documents (PDF, DOCX, text), and later
video. Windows-first. Free, MIT, no telemetry, no accounts.

The strategic wedge is **format coverage**: every existing competitor owns one
format (screenshots only, or images only, or images+video). Tessera's demo is
one query returning results from several formats at once.

## Current phase

**Week 2–3: engine build, ticket by ticket from `.scratch/tessera-v0.1/issues/`.**
Ticket 01 (store + text pipeline + typo-tolerant lexical search + CLI) is
resolved. The frontier is the image/document pipelines (tickets 02–04), which
may only start once the store and the Pipeline seam are proven — they are.

## Glossary

- **Tile** — one indexed item: an image, or a document page/chunk. From
  *tessera*, a mosaic tile.
- **Corpus** — the set of folders the user has chosen to index.
- **Arm** — one search channel: lexical (FTS5), image (CLIP), text (MiniLM).
  A query runs up to three arms and fuses their rankings.
- **Fusion / RRF** — Reciprocal Rank Fusion. Merges arm rankings by rank
  position rather than raw score, because BM25 and cosine scores are not
  comparable quantities.
- **Spike** — the week-1 throwaway experiment that measures model latency on
  real hardware before the engine is built. Throwaway code, real numbers.
- **Kill criteria** — pre-committed stop conditions (ADR-0005). Decided before
  building so that stopping later is cheap and unemotional.
- **Placeholder (OneDrive)** — a cloud file present locally only as metadata.
  Never hydrated by Tessera, always skipped.
- **Pre-alpha** — before first release: no performance claims, no feature
  claims, roadmap items stay unchecked.
- **Red ocean** — a solution space with several shipped competitors, where
  craft and positioning decide rather than the feature list.
