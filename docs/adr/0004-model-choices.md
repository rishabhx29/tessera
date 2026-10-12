# ADR-0004: Model choices and first-run model delivery

- Status: Accepted — the OCR arm and the image-arm clause are superseded by
  [ADR-0006](0006-week-1-spike-model-decisions.md), which activated this ADR's
  own named fallback after the week-1 spike.
- Date: 2026-10-11

## Context

The engine needs OCR, image↔text embeddings, and text embeddings. Model choice
drives three user-visible things: index speed on old CPUs, memory footprint,
and search quality. The models also determine whether the "runs on any PC, no
GPU" positioning is true.

## Decision

- **OCR**: RapidOCR (PP-OCRv5 mobile models) via ONNX Runtime.
- **Image arm**: MobileCLIP2-S0 (or SigLIP-base, decided by spike quality) —
  int8-quantized ONNX.
- **Text arm**: all-MiniLM-L6-v2 — int8-quantized ONNX.
- **Delivery**: models are **not** in the installer. They download once on
  first run, each verified against a pinned SHA-256 checksum.
- **Language**: English at launch; a Devanagari (Hindi/Marathi/Nepali) OCR pack
  is a v0.1.x fast-follow behind a settings toggle.

## Alternatives considered

- **Windows.Media.Ocr** — fastest and most accurate on Windows, but
  Windows-only and it would fork the engine by platform. **Nominated as the
  fallback if the spike shows RapidOCR is too slow on CPU** — the pipeline
  seam keeps this a contained swap.
- **Shipping models inside the installer** — balloons a ~15 MB installer to
  ~200 MB; also puts us in the redistribution-licensing business.
- **Large VLMs for captioning (Moondream-class)** — better descriptions,
  seconds-per-image on CPU. Rejected for v0.1; revisit for auto-organize
  (v0.4), where latency matters less than quality.

## Consequences

- First run needs ~200 MB of downloads and a visible setup step (checksum
  display, like the best neighbors do).
- All three models are quantized int8: quality loss is acceptable at personal
  scale, and old hardware stays viable.
- Changing any model version invalidates that arm's vectors — recorded in the
  `meta` table, forcing re-index rather than silent quality drift.
- Licenses are all permissive (Apache-2.0/MIT); nothing in the delivery path
  conflicts with ADR-0002.
