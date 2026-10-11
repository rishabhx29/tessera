# Performance

> **No numbers yet.** The week-1 spike has not run. This file fills with
> measured numbers and methodology *before* any public claim is made — that
> rule is part of the project (README, "Rules this project holds itself to",
> ADR-0005).

## Targets (from the product definition)

| Metric | Target | Why it matters |
|---|---|---|
| Query end-to-end @ 50k items | < 100 ms | "Instant" is the product |
| Image saved → searchable | < 1.5 s | The demo's second half |
| Document saved → searchable | < 300 ms | Backlog feel |
| Idle RAM | < 250 MB | Old hardware is the beachhead |
| Disk per 1,000 items | ~25 MB | Nobody indexes 50 GB twice |

## Spike methodology (week 1)

Measured on the developer's real machine, on real files — not synthetic
benchmarks:

1. **Corpus**: 1,000 of the developer's own recent screenshots + 200 real
   PDFs/DOCX/text files. Nothing downloaded or generated for the occasion.
2. **Warm-up excluded**: cold start (model load) reported separately from
   steady-state throughput.
3. **Per-stage numbers**: decode, OCR (p50/p95), CLIP embed, MiniLM embed,
   store write — individually, then end-to-end.
4. **Query**: lexical, image-arm, text-arm, and fused — at 1k / 10k / 50k tiles.
5. **Hardware**: CPU model, RAM, disk, Windows version recorded verbatim.
6. **Repeatable**: the spike code lives on a `prototype/spike` branch so any
   number in this file can be re-derived.

## Results

_Filled in after the spike. Format: measured value, hardware, method link._
