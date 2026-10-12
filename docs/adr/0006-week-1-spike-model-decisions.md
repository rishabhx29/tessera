# ADR-0006: Week-1 spike model decisions

- Status: Accepted
- Date: 2026-10-12
- Supersedes: the OCR choice and the image-arm "decided by spike" clause in
  [ADR-0004](0004-model-choices.md), which pre-named this exact fallback path.

## Context

The week-1 spike measured the ADR-0004 configuration on real hardware and
**failed the latency targets** (see `docs/performance.md`): RapidOCR PP-OCRv5
at p50 2,500–8,000 ms per image is ~10× the entire save→searchable budget.
Remedy experiments ran the same measurements on this machine's 40 real
screenshots:

| Candidate | latency | quality |
|---|---|---|
| Windows.Media.Ocr | p50 50 ms | 188 mean chars; meaningful miss in 3/40 files |
| RapidOCR PP-OCRv5 int8 | p50 3,256 ms | 285 mean chars |
| SigLIP-base int8 (image) | 200 ms embed / **40 ms query** | top-1 0.90 synthetic, 0.50 real |
| CLIP ViT-B/32 int8 (image) | **85 ms embed** / 70 ms query | top-1 0.05 synthetic, 0.46 real |
| MobileCLIP2-S0 int8 | — | unusable: ConvInteger(10) not implemented by onnxruntime CPU EP |

## Decision

1. **OCR arm: Windows.Media.Ocr** for v0.1. RapidOCR's extra characters do not
   justify 65× the latency; v0.1 is Windows-first, so the WinRT dependency is
   free. RapidOCR remains the cross-platform candidate for the macOS/Linux
   phase, and an optional background "enrichment pass" (v0.1.x idea, not a
   commitment).
2. **Image arm: SigLIP-base int8, retained.** Queries run on every keystroke;
   indexing runs at idle priority in the background. Query latency (40 ms vs
   70 ms) and fine-text matching (0.90 vs 0.05 synthetic top-1) beat CLIP32's
   faster indexing.
3. **Text arm: all-MiniLM-L6-v2 int8, unchanged** (35 ms query, fine).
4. **Query execution: the three arms run in parallel** (threads; onnxruntime
   releases the GIL). Measured sequential fused was 159 ms; parallel projects
   to ~50 ms — under the 100 ms target.
5. MobileCLIP2-S0 as an int8 ONNX artifact is rejected until an
   onnxruntime-compatible export exists.

## Consequences

- The engine's OCR pipeline gains a Windows implementation behind the
  existing `Pipeline` seam; the cross-platform OCR story moves to a later
  phase decision, not v0.1.
- Model download set changes: WinOCR needs no download (ships with Windows);
  SigLIP and MiniLM stay as first-run, checksum-pinned downloads.
- Document extraction remains over its sub-target (~1,000 ms vs 300 ms,
  pypdf-bound) and is explicitly accepted for v0.1 — docs are a minority of
  the corpus and indexing is background work. Revisit during dogfooding.
- All numbers and methodology live in `docs/performance.md`; the scripts that
  produced them are committed on `prototype/spike`.
