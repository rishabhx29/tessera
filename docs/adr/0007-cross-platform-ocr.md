# ADR-0007: Cross-platform OCR — platform-native fast path with a shared ONNX fallback

- Status: Accepted
- Date: 2026-10-12
- Refines: [ADR-0006](0006-week-1-spike-model-decisions.md) (which chose
  Windows.Media.Ocr for v0.1)

## Context

ADR-0006 picked Windows.Media.Ocr on measured evidence (p50 50 ms vs 3,256 ms
for RapidOCR). That engine is WinRT — Windows-only. The product requirement is
explicit: **one app anyone can run on their PC, fast on every OS**, in the
spirit of gyotaku (Rust, all platforms, ~0.77 s save→searchable — measured on
OCR alone, without any image embeddings).

Platform reality:

- **Windows**: Windows.Media.Ocr, measured 50 ms p50 on this machine.
- **macOS**: Apple's Vision framework (`VNRecognizeTextRequest`) is the native
  fast path, reachable from pure Rust via the maintained `objc2-vision` crate
  (145k downloads, feature-gated `VNRecognizeTextRequest`).
- **Linux**: no system OCR. The cross-platform fallback is the ONNX engine
  (RapidOCR PP-OCRv5 via `ort`), which is also the *highest-quality* extractor
  we measured (285 vs 188 mean characters) but 65× slower.

## Decision

OCR sits behind an `OcrEngine` trait (the same seam style as the format
`Pipeline` trait in `docs/architecture.md`), with three implementations:

1. **WindowsOcr** — Windows.Media.Ocr via the `windows` crate. *Measured:
   p50 50 ms, p95 178 ms on 40 real screenshots.*
2. **VisionOcr** — macOS Vision via `objc2-vision`. *Not yet measured — must be
   measured on a Mac before any public claim (honesty rule in README).*
3. **OnnxOcr** — RapidOCR PP-OCRv5 via `ort`, cross-platform. *Measured:
   p50 2,655–3,256 ms on screenshots; the most accurate text.*

Selection rules:
- Platform default = the native engine (1 or 2). Users can override in
  settings.
- The ONNX engine is the cross-platform fallback and, optionally, a
  background "quality pass" for files the native engine found little text in
  (v0.1.x idea, not a v0.1 commitment).

## Consequences

- No new architecture: this is one more implementation behind an existing
  seam. The cross-platform promise is preserved at speed on Windows and macOS,
  and at quality-but-slower on Linux — where indexing is background work
  anyway.
- Honest positioning: on Linux, Tessera's OCR is in gyotaku's class (slower
  background indexing), not in Windows-native class. Say so plainly in the
  README rather than implying uniform speed.
- CI grows a matrix (windows-latest, macos-14, ubuntu-latest) so adapters are
  compiled and smoke-tested everywhere. macOS/Linux latency numbers enter
  `docs/performance.md` only when someone runs the spike scripts on those
  machines.
- Week 2–3 scope grows by roughly one small adapter per non-Windows platform
  (the user accepted 2–3 extra weeks for cross-platform parity).

## Amendment (ticket 06 — the macOS adapter landed)

`VisionOcr` is built and cfg-gated to macOS, behind the same seam. Two things
this record that the original decision did not settle:

- **`VNRequestTextRecognitionLevel::Fast` is set, and that choice is
  unfounded.** It was picked on the reasoning that background indexing should
  not spend minutes of battery on marginal accuracy gains for crisply-rendered
  text. Nobody has measured `Fast` against `Accurate` on a real screenshot
  corpus. Whoever measures macOS latency next should measure both levels and
  record the difference, because until then this is a guess in the code and a
  guess is not a result.
- **There is still no macOS latency number, and the "fast on every platform"
  promise is currently a promise about two platforms.** The adapter is proven
  to *work* — a macOS CI runner reads a committed fixture and asserts non-empty
  text — and nothing is known about how fast it is. `docs/performance.md` says
  "not yet measured" rather than carrying a number.

The ADR's CI-matrix consequence is now partly discharged: `windows-latest`,
`ubuntu-latest` and `macos-14` all run fmt / clippy / test. That matters more
than the three jobs cost, because `ocr/vision.rs` is `#[cfg(target_os =
"macos")]` — the Windows and Linux runners cannot see that code at all, so
without the macOS runner the adapter would be plausible-looking source that no
machine has ever compiled.
