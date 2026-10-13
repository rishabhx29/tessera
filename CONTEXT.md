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
Tickets 01 (store + text pipeline + typo-tolerant lexical search + CLI), 02
(change detection, skip rules, diagnostics), 03 (pinned model downloads,
checksum verification, per-arm version pinning) and 04 (the OCR seam, the
Windows native adapter, the image pipeline) are resolved. Screenshots are
searchable by the text inside them: measured p50 43.8 ms per image
(`docs/performance.md`). The frontier is ticket 05: the ONNX fallback, which is
what makes OCR work on Linux and gives the quality pass.

## Glossary

- **Tile** — one indexed item: an image, or a document page/chunk. From
  *tessera*, a mosaic tile.
- **Corpus** — the set of folders the user has chosen to index.
- **Root** — one indexed folder. The corpus is the set of roots; each file row
  records the root it came from, which is what scopes deletion propagation.
- **Arm**'s **version** — the model that produced an arm's vectors. Recorded in
  `meta` beside the vectors; a mismatch means the arm is outstanding work, never
  that old vectors may be blended with new ones.
- **Arm** — one search channel. Three exist: **lexical** (FTS5), **image** (SigLIP),
  **text** (MiniLM). A query runs up to three arms and fuses their rankings.
  Only the two embedding arms hold vectors; the lexical arm has none, and OCR
  belongs to no arm at all — it produces text, which the lexical arm indexes.
- **Pin** — the SHA-256 a model file must hash to before the engine will use it,
  compiled into the binary (ADR-0009). A pin that lives beside the bytes it
  checks is not a pin.
- **Fusion / RRF** — Reciprocal Rank Fusion. Merges arm rankings by rank
  position rather than raw score, because BM25 and cosine scores are not
  comparable quantities.
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
