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
checksum verification, per-arm version pinning), 04 (the OCR seam, the
Windows native adapter, the image pipeline) and 05 (the cross-platform
PP-OCRv6 fallback and automatic engine selection) are resolved and committed.
Screenshots are searchable by the text inside them, through either engine:
measured p50 34.4 ms and 188 characters for the native engine, 949.6 ms and 288
characters for the fallback, which also read the one screenshot in the corpus
the native engine missed entirely (`docs/performance.md`).

Tickets 01–05 were re-audited against their own acceptance criteria rather than
against their own summaries. Two real defects were found and fixed — a failed
file's reason that dropped the cause chain, and an atomicity guarantee asserted
in prose with no test behind it — and both fixes are pinned by tests that were
verified to fail against the old behaviour.

**The frontier is ticket 12.** Tickets 06–11 are also resolved: the macOS
Vision adapter (below), the SigLIP image arm for description search, the PDF
and DOCX pipelines (page-aware tiles), the MiniLM text arm, ticket 10 (weighted
RRF fusion of the three arms), and ticket 11 (`tessera watch`: Windows idle
priority, newest-first queue, battery pause, up to 4 workers, and transactional
crash-resume). What remains in weeks 2–3: **ticket 12** (the mixed-format
milestone gate). The description-search objective set in the product definition
is still unmet; its measured state and the one deliberate, unshipped lever are
recorded under "Where this leaves the feature" in `docs/performance.md`.

Ticket 06 (the macOS Vision adapter) landed behind the same seam, cfg-gated to
macOS and proven to work on a `macos-14` runner. **It has no latency number and
none may be published** — `docs/performance.md` records "not yet measured", and
the `Fast` recognition level it requests is a hypothesis that has never been
measured against `Accurate`. Linux is a supported target on the ONNX fallback by
design (ADR-0007), not a platform that fails to build.

## Glossary

- **Tile** — one indexed item: a numbered chunk of a document, or the whole
  extracted text of an image — `seq`, `text`, and a 1-based `page` when the
  format has pages. From *tessera*, a mosaic tile.
- **Corpus** — the set of folders the user has chosen to index.
- **Root** — one indexed folder. The corpus is the set of roots; each file row
  records the root it came from, which is what scopes deletion propagation.
- **Watch mode** — `tessera watch [folder]...`: background indexing that keeps
  the corpus fresh while the machine is in use. Watches the named folders, or
  every root the store already owns. Its event feed, power source and process
  priority are platform adapters behind traits; the loop itself is portable.
- **Backlog** — the watcher's queue of files owed work: newest-first on mtime,
  one live entry per path (a re-enqueued path moves rather than duplicates).
- **Drain** — one stretch of work between two idle points. The models load when
  the backlog is non-empty and are released when it empties: per drain, never
  per file, because a machine at idle must not hold ~250 MB of weights.
- **Attempt** — one pass over a file that ended without extracting it
  (`skipped`, `failed`, `awaiting_ocr`); its count drives the watcher's **retry
  backoff** (five minutes, doubling to an hour). A changed file is new work, not
  a retry, and an extraction clears the count. "Eventually retried" is a promise
  the backoff keeps; "retried on every rescan" is the bug it fixes.
- **Schedule** — when the watcher may take work: `idle-only` (the default; the
  queue pauses on battery and resumes on AC), `always`, or `paused`. What pauses
  is the queue, never a worker holding a file.
- **Arm**'s **version** — the model that produced an arm's vectors. Recorded in
  `meta` beside the vectors; a mismatch means the arm is outstanding work, never
  that old vectors may be blended with new ones.
- **Arm** — one search channel. Three exist: **lexical** (FTS5), **image** (SigLIP),
  **text** (MiniLM). A query runs all three and fuses their rankings (ticket 10);
  `--describe` and `--describe-text` still run a single arm, as diagnostics.
  Only the two embedding arms hold vectors —
  one per image file, one per tile — and the lexical arm has none. OCR belongs
  to no arm at all: it produces text, which the lexical arm indexes and the
  text arm embeds like any other tile.
- **Pin** — the SHA-256 a model file must hash to before the engine will use it,
  compiled into the binary (ADR-0009). A pin that lives beside the bytes it
  checks is not a pin.
- **Fusion / RRF** — Reciprocal Rank Fusion. Merges arm rankings by rank
  position rather than raw score, because BM25 and cosine scores are not
  comparable quantities. Weighted since ticket 10: an arm's vote is scaled by
  the file's **evidence** — Full (a text-bearing file): lexical 1.0, image
  0.05, text 1.0; Little (a few axis labels): 0.5 / 0.5 / 0.5; None (no tiles,
  a photo): 0.0 / 1.0 / 0.0. The Full row is measured; the other two are
  defaults until something measures them, and all three live in `CHOSEN`
  (`crates/core/src/fusion.rs`).
- **Evidence** — the fusion class of a file: how much summed tile text the
  index holds for it (Full, Little, None — above). A photo is exactly where
  the image arm should be trusted; a text-bearing file is where an equal image
  voice measured away 59 points of top-1 (`docs/performance.md`).
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
