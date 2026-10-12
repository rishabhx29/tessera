# Architecture Decision Records

Decisions that are expensive to reverse. Each ADR is short: context, decision,
alternatives rejected, consequences. Supersede with a new ADR, never by
editing the old one in place.

| ADR | Decision |
|---|---|
| [0001](0001-rust-and-tauri.md) | Rust core with a Tauri shell |
| [0002](0002-mit-license.md) | MIT license |
| [0003](0003-hybrid-search-three-arm-rrf.md) | Hybrid search: three arms fused with RRF |
| [0004](0004-model-choices.md) | Model choices and first-run model delivery |
| [0005](0005-phased-rollout-and-kill-criteria.md) | Phased rollout and pre-committed kill criteria |
| [0006](0006-week-1-spike-model-decisions.md) | Week-1 spike model decisions (OCR → Windows.Media.Ocr; SigLIP retained) |
| [0007](0007-cross-platform-ocr.md) | Cross-platform OCR: platform-native fast path (Windows.Media.Ocr / macOS Vision) + shared ONNX fallback |

Statuses: 0001–0003, 0005 **Accepted** (2026-10-11); 0004 **Accepted,
partially superseded by 0006**; 0006, 0007 **Accepted** (2026-10-12).
