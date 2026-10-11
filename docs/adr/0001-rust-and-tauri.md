# ADR-0001: Rust core with a Tauri shell

- Status: Accepted
- Date: 2026-10-11

## Context

Tessera needs: file watching, image decoding, ONNX inference (OCR + CLIP +
text embeddings), SQLite with full-text and vector search, a desktop UI with a
global hotkey, and small installers — Windows first, other platforms later.

## Decision

Rust for the engine (`tessera-core`), Tauri (React + TypeScript) for the
desktop shell (`tessera-app`).

## Alternatives considered

- **C#/.NET + WinUI** — first-class Windows APIs (Windows.Media.Ocr is one
  call away) and a fast path if we were .NET-native. Rejected because the
  cross-platform story later is weak, and the OSS tooling around local AI
  inference (ONNX Runtime embeddings, sqlite-vec) is most mature in Rust.
- **Python core + sidecar** — fastest to prototype. Rejected for the shipping
  product: installers, memory footprint, and process model all fight the
  "fast and legitimate" positioning. Python stayed for the week-1 spike only.

## Consequences

- One language across the engine; the UI is ordinary web development, which is
  where most velocity comes from.
- Every dependency the pipelines need exists: `ort` (ONNX Runtime),
  `rusqlite` + sqlite-vec, `notify`, `image`, `walkdir`.
- Rust's learning curve is real and is the main build risk. Mitigation: the
  engine is built format-by-format with a CLI before any UI exists, and the
  week-1 spike isolates the hardest technical unknown first.
- Cross-platform (macOS/Linux) stays cheap later: ONNX models are portable,
  unlike the Windows-only OCR API.
