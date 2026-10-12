# ADR-0008: Final OCR stack and engine integration notes

- Status: Accepted
- Date: 2026-10-12
- Refines: [ADR-0007](0007-cross-platform-ocr.md) — same architecture, now
  with the fallback engine chosen and measured.

## Context

ADR-0007 set the architecture (native fast path per OS + one shared ONNX
fallback) but left the fallback as PP-OCRv5 via RapidOCR. Two questions
remained: what exactly is the best cross-platform engine, and is the native
path still worth its quality trade-off. A three-way bake-off ran on this
machine's 40 real screenshots (`spike/ocr_bakeoff.py`):

| Engine | p50 | mean chars |
|---|---|---|
| Windows.Media.Ocr | 35 ms | 188 |
| RapidOCR PP-OCRv5 (defaults) | 2,820 ms | 267 |
| PP-OCRv6 det-tiny + rec-small (gyotaku tuning) | 2,412 ms | 287 |

## Decision

1. **Windows default: Windows.Media.Ocr** (unchanged from ADR-0006).
2. **Cross-platform fallback and opt-in quality pass: PP-OCRv6 det-tiny +
   rec-small ONNX** (the exact artifacts gyotaku pins: RapidOCR's ModelScope
   exports, checksums recorded in `spike/model-lock.json`). Best extraction
   quality (287 chars), unified 18,710-character 50-script charset, and
   proven-in-production (gyotaku ships it cross-platform).
3. **macOS: Vision** (unchanged from ADR-0007; measure on a Mac before
   claiming numbers).
4. The v6 engine is also the designated background **quality pass** for the
   3-in-40 files where the native engine finds little text (v0.1.x option).

## Engine integration notes (hard-won, for the Rust implementation)

These came out of the spike's debug loop and are the difference between a
working engine and a week of confusion:

- **The rec export emits per-step probabilities, not logits** — each row sums
  to 1.0000. Applying softmax again crushes a 0.94 confidence into 0.0001,
  and every line fails the score gate. Detect normalization and use raw.
- **Alphabet mapping**: class 0 is the CTC blank; the *final* class is the
  space character that `splitlines()` drops from the metadata. Correct
  mapping is `['blank'] + charset_lines + [' ']`.
- **Input order is RGB**; cv2-style BGR decodes garbage.
- **Detection**: axis-aligned connected components on the probability map at
  threshold 0.3 works for screen text (no rotated-box DB postprocessing
  needed), matching gyotaku's finding.
- **Score gate 0.5** cleanly separates real text (0.98–1.00) from icon
  misreads (≤0.73).
- **Batch recognition by total width**, because the 18,710-class output
  tensor dominates memory and time; unbounded batching is the slow path.

## Consequences

- The fallback's honest number on this hardware class is ~2.4 s/screenshot in
  the Python harness; the Rust engine targets gyotaku's class (~0.7 s on
  comparable cores) via width-batching and Level-3 graph optimization. Both
  numbers go in `docs/performance.md`; the README claims neither until the
  Rust engine is measured.
- Two engines ship in v0.1 (native + ONNX fallback); the quality pass is a
  settings toggle, not a default.
- The 18,710-class rec model means indexing throughput on Linux-class hardware
  is bounded by the recognizer; the watcher's idle-priority, newest-first
  queue design absorbs this.
