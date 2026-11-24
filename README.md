# Tessera

**One search bar for everything your PC can't search: images, screenshots, PDFs and documents. Offline, free and open source.**

[![Release](https://img.shields.io/github/v/release/rishabhx29/tessera?color=C2410C)](https://github.com/rishabhx29/tessera/releases)
[![CI](https://github.com/rishabhx29/tessera/actions/workflows/ci.yml/badge.svg)](https://github.com/rishabhx29/tessera/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-16140F.svg)](LICENSE)
[![Website](https://img.shields.io/badge/website-tesseraweb.vercel.app-5E7358.svg)](https://tesseraweb.vercel.app)

A *tessera* is one tile in a mosaic. Every screenshot, photo, page and document on your PC is a fragment. Tessera is the picture they add up to.

Type what you remember, not the filename. Tessera finds the screenshot by the words inside it, the photo by what it shows, and the document by what it says. It runs on your machine and never uploads a file.

## See it

<img src="website/public/media/app-results.webp" alt="Tessera showing picture results for the search 'a dog in a blanket'" width="820">

Search *a dog in a blanket* and the picture comes back, with the reason it matched.

## Install

**[Download Tessera for Windows (x64)](https://github.com/rishabhx29/tessera/releases/latest/download/Tessera_x64-setup.exe)** · [All releases](https://github.com/rishabhx29/tessera/releases)

1. Run `Tessera_x64-setup.exe`. It installs for the current user, so Windows does not ask for an administrator.
2. The installer is not code-signed yet. If SmartScreen asks, choose **More info**, then **Run anyway**.
3. On first launch Tessera downloads its on-device models once, about 1.6 GB, and checks each file against a SHA-256 pin built into the binary. This is the only network access the app makes.
4. Choose a folder. Tessera reads it in the background.
5. Press **Ctrl+Alt+T** from anywhere to search.

Nothing else to install. There is no account, no key and no separate model package.

**Requirements:** Windows 10 or 11 on x64. WebView2, which ships with Windows 11 and current Windows 10; if it is missing, Tessera offers the Microsoft download. About 1.7 GB of disk for the models, plus the index (about 14 MB per 1,000 files).

**Uninstall:** Settings > Apps > Installed apps > Tessera. Your models, index and settings live in `%LOCALAPPDATA%\TesseraData`; remove that folder to reclaim the space.

**Checksums:** each release carries `SHA256SUMS.txt` and `Tessera_x64-setup.exe.sha256`. Verify with `Get-FileHash Tessera_x64-setup.exe -Algorithm SHA256`.

## What it searches

| Kind | Formats | How it is read |
|---|---|---|
| Pictures | PNG, JPG, JPEG, WEBP, BMP, GIF, TIFF | OCR for the words, a local vision model for what the picture shows |
| Documents | PDF | The text layer, page by page. A PDF with no text layer is recorded as waiting for OCR, not dropped |
| Documents | DOCX | The document text |
| Text | TXT, MD, CSV, LOG | Chunked, with overlap, so a phrase across a boundary is still found |

Four lenses sit above the results. **All** mixes everything and says why each file matched. **Pictures** searches by what a picture looks like. **Words** searches the text inside pictures. **Documents** searches the text inside files.

Everything is one index in a single SQLite file. A moved file keeps its row, a changed file is re-read, and a cloud-only file is skipped rather than half-read.

## Privacy

- No telemetry, no accounts, no ads, no paid tier.
- No network calls at query time. Search is local.
- The only network access is the first-run model download, checksum-verified against pins compiled into the binary.
- The index is your data and stays in your data folder. Nothing leaves the machine.
- MIT licensed. See [SECURITY.md](SECURITY.md) for the full promise.

## Measured numbers

These are measurements from [`docs/performance.md`](docs/performance.md), not targets. The test machine: Intel Core Ultra 5 225H (14 logical processors), 15 GB RAM, Windows 11 build 26200, mains power, no build running, warm OS file cache. Corpus: 5,000 files, 17,807 chunks, 1,800 images, 0 failed. Method: the gate's five-query sweep, 32 samples for the fused figure.

| What | p50 | p95 |
|---|---|---|
| Fused search at 5,000 files | **65 ms** | **80 ms** |
| Keyword arm | 17 ms | 58 ms |
| Picture arm | 36 ms | 46 ms |
| Meaning arm | 44 ms | 60 ms |

At 50,000 files the fused search does **not** meet the 100 ms target: 457 ms p50 and 523 ms p95, mostly in the meaning arm. That row is recorded as a miss. The index for 50,000 files is 686 MB, and the initial drain runs at 10.8 files per second.

Caveats, all from the log: one machine, one run per configuration, a corpus that is about 31% real files (generated screenshots are the easy OCR case), and p95 over 32 samples is close to the maximum rather than a distribution estimate. Numbers are never copied between machines or scaled by wishful arithmetic.

## Status and limits

Tessera is at **v0.1.0**, the first public release, and still early.

- Windows x64 only for the installer. The engine also compiles and tests on Linux and macOS; the desktop shell is Windows-first.
- No video. Recorded meetings and screen recordings arrive in v0.2.
- Scanned PDFs have no text layer and are counted, not indexed, until the scheduled OCR pass.
- XLSX and PPTX are planned, not supported.

The roadmap and its kill criteria live in [ADR-0005](docs/adr/0005-phased-rollout-and-kill-criteria.md). A roadmap item is checked off only when a release contains it.

## Build from source

Prerequisites: Rust stable, Node.js 20 or newer, and on Windows the MSVC build tools. The engine downloads a pinned ONNX Runtime during the build.

```sh
git clone https://github.com/rishabhx29/tessera
cd tessera

# engine CLI
cargo run -p tessera-core

# desktop shell (installs the Tauri CLI on first use)
cd crates/app
npx @tauri-apps/cli dev
```

To build the installer, run the Tauri build:

```sh
cd crates/app
npx @tauri-apps/cli build
```

The installer lands in `target/release/bundle/nsis/`. On MSVC the build links ONNX Runtime statically, so the installer needs no extra dll. A GNU build that links the dll dynamically (`ORT_LIB_LOCATION`) must copy `onnxruntime.dll` beside `Tessera.exe` before bundling.

Gates that must pass before a change merges:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

## Documentation

- [Website](https://tesseraweb.vercel.app): download, FAQ and the privacy page
- [Product definition](docs/product-definition.md): the locked spec
- [Architecture](docs/architecture.md): crates, data model, per-format pipelines
- [Performance log](docs/performance.md): every measured number, with method
- [ADRs](docs/adr/): decisions that are expensive to reverse
- [Shared vocabulary](CONTEXT.md): the terms this project uses
- [Agent guide](agents.md): commands and phase rules for AI coding agents

## Contributing

Bug reports and testing plans are the most useful help right now. Read [CONTRIBUTING.md](CONTRIBUTING.md) first. Hard-to-reverse decisions start as discussion and an ADR, not a PR. Participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md).

## Acknowledgements

Tessera stands on open models and open code: the ONNX Runtime, SigLIP for pictures, MiniLM for text, ViT-GPT2 for picture captions, PP-OCRv6 for the cross-platform OCR fallback, and the Windows OCR API for the fast path. Full pins and sources are in [`crates/core/src/models/lock.json`](crates/core/src/models/lock.json).

## License

[MIT](LICENSE).
