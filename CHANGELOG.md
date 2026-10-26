# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/) once
releases begin.

## [Unreleased]

### Added

- CI now also runs on Linux and macOS. Linux is where the non-Windows paths are
  compiled at all; macOS is the only place `ocr/vision.rs` exists, since it is
  `#[cfg(target_os = "macos")]` and no other runner can see that code. The
  checksum job re-verifies the pinned model checksums against upstream — a step
  that was `#[ignore]`d and therefore never ran.
- The macOS OCR adapter: Apple's Vision framework behind the same `OcrEngine`
  seam, so screenshots are searchable on a Mac at native speed with no
  language pack and no download. Smoke-tested on a macOS runner against a
  committed fixture; **not yet measured**, so no speed is claimed.
- Repository scaffold: MIT license, Contributor Covenant, security policy,
  issue templates, PR template, CI (fmt / clippy / test).
- Product Definition v1.0, architecture notes, and five ADRs (stack, license,
  search design, model choices, rollout plan and kill criteria).
- Rust workspace skeleton (`tessera-core`, `tessera-app`) with green CI.
- The engine's first path: SQLite store (WAL, FTS5 trigram over an
  external-content tiles table), text pipeline, typo-tolerant lexical search,
  and the `tessera index` / `search` / `stats` CLI.
- Incremental indexing: change detection by size + mtime, falling back to a
  streamed sha1 only when the cheap evidence disagrees. Re-indexing an
  unchanged folder reads nothing.
- Move and rename detection: a new path whose content hash matches a row under
  the same folder moves that row instead of extracting the same bytes twice,
  and re-titles the searchable text to the file's new name.
- Deletion propagation: rows whose file is gone from disk are dropped, scoped
  to the folder that was indexed.
- Skip rules recorded with reasons — cloud placeholders (Windows Files
  On-Demand attributes, never hydrated) and files above the 50 MB cap — plus a
  skip-reason breakdown in `tessera stats`.
- Store schema v2 (corpus `root` column, `root`+`sha1` index) with a tested
  v1 → v2 migration.
- Pinned model delivery (`tessera models install` / `models status`): SigLIP,
  MiniLM and PP-OCRv6 download once, each verified against a SHA-256 compiled
  into the binary, with mirrors tried in order and no weakening of the pin.
- Downloads are atomic — a temporary name while hashing, renamed into place only
  after verification — and interrupted ones are swept on the next run.
- Per-arm vector-space versioning in `meta`: changing a model's pin marks that
  arm's vectors as needing re-embedding, and only that arm.
- `--models-dir` overrides where models live.
- The OCR seam (`OcrEngine`) and the Windows adapter: text inside screenshots is
  now searchable. Measured p50 43.8 ms per image on the real corpus.
- The image pipeline: png/jpg/jpeg/webp/bmp/gif/tiff decode and OCR to one tile
  each; images with no text are indexed rather than failed; a decode failure is
  recorded against that file with its reason and the run continues.
- `--ocr-engine` overrides the OCR engine, which defaults to the platform's own.
- `tessera stats` lists each failed file with the reason the engine gave.
- The cross-platform OCR engine (PP-OCRv6 on ONNX Runtime) behind the same seam:
  OCR now works on any platform, including Linux, with no language pack. Models
  come from the same pinned manifest as everything else
  (`tessera models install --set ppocr`, 23 MB).
- Engine selection falls back automatically: the platform engine if there is one,
  then the cross-platform engine, and the error tells you how to fix it if
  neither. An engine you named and did not get still stops the run.
- `tessera ocr <image> [--engine native|onnx] [--scores]` prints the text and the
  timing, with each line's confidence, so the two engines are comparable from a
  terminal.
- Measured on this machine's 40-screenshot corpus: native p50 34.4 ms / 188
  characters; fallback p50 949.6 ms / 288 characters, and it read the one image
  the native engine found no text in at all.
- `tessera watch [folder]...`: background indexing that keeps the corpus fresh
  while the machine is in use, so a file saved now is searchable in seconds. With
  no folder it watches every root the store already owns.
- The watch queue is newest-first: a file saved while a large backlog is
  processing is indexed before the backlog, and one live entry per path keeps a
  save's several filesystem events from becoming several extractions.
- Watch workers share the per-file pass `tessera index` runs
  (`index::process_one`), up to four of them, each with its own WAL connection
  and a busy timeout rather than a dropped file.
- Events are a trigger, not the record: every event re-stats the path, a notify
  overflow is treated as a full rescan, and a 60 s rescan re-walks each root.
- Watch mode runs at Windows idle priority (`IDLE_PRIORITY_CLASS`, worker
  threads at the lowest priority). Other platforms are unchanged, and
  `docs/architecture.md` says so instead of pretending otherwise.
- Battery policy: the queue pauses on battery under the default `--schedule
  idle-only` and resumes on AC; `--schedule always` ignores the power state and
  `--schedule paused` starts paused. A worker always finishes the file it holds.
- Models load when there is work and are released when the queue drains. The
  pinned sets are verified once per watch process and sessions are rebuilt per
  drain, so a batch does not re-hash a gigabyte and an idle machine does not hold
  the weights.
- Ctrl-C stops the watcher after the file in flight. Restarting re-stats every
  file and reports it unchanged rather than re-extracting it, which is the same
  resume path a crash takes.
- A file that will not extract is retried on a backoff — five minutes, doubling
  to an hour — or as soon as its bytes change, instead of on every 60 s rescan.
  A permanently unreadable file no longer re-opens on every walk, while "never
  extracted" still means "eventually retried"; `tessera index` is an explicit
  request and still retries every time.
- Store schema v6 (`files.attempts`), the consecutive non-indexed attempt count
  the retry backoff is computed from, with a tested v5 → v6 migration.
- `scripts/measure-watch.ps1` commits the harness that produced the watch-mode
  numbers in `docs/performance.md`, so they can be reproduced rather than
  trusted.

### Changed

- `tessera index` reports indexed / unchanged / moved / skipped / failed /
  removed counts.
- `tessera watch` parks on the nearest timer it keeps — a rescan, the power
  poll, a retry — capped at one second, instead of waking ten times a second on
  a fixed 100 ms tick. A delivered event still wakes it immediately, so a save
  is read as soon as notify delivers it.
- A coordinator store error no longer ends the run: it is reported, a rescan is
  owed, and the call is retried on a growing backoff, so a transient lock
  (another `tessera index`, a WAL checkpoint) does not kill a watcher meant to
  run for days. Twenty consecutive failures still stop it, because that is not
  transient.
- The `tessera watch` stop summary prints unchanged files, walk errors, files
  that could not be recorded, failed index updates and files waiting for their
  retry, so a run that did less than it looks like cannot end on a clean line.

### Fixed

- A sustained store outage no longer burns a core while the watcher waits it
  out. The retry after a failed coordinator call slept for the *nearest timer*,
  which the loop reports as zero once a timer has come due (the 30 s power poll,
  the 60 s rescan) — and a zero wait is an immediate return, so every backoff
  longer than the tick spun at millions of passes a second for the rest of it.
  Measured with a table renamed away under a live watcher: 170.8 s of CPU over
  the first 371 s before, 0.17 s of CPU in the next six minutes after, on the
  same 1/2/4/…/60 s backoff. The wait is now the rest of the backoff capped by
  the one-second shutdown tick, and a test asserts the loop never asks the event
  feed for a zero wait while a retry deadline is still in the future.
- An ONNX Runtime that will not start now names the runtime it loaded and the
  fix. A build whose pinned 1.23 `onnxruntime.dll` is not beside the executable
  binds to the older copy Windows keeps in `System32`, which does not answer the
  pinned API level, and `ort` reported it as a bare panic — "Failed to initialize
  ORT API" — with no version and no file. The refusal now reports the loaded
  version and points at the fallback.
- ONNX bindings are pinned to API level 23, the level of the runtime this build
  links against. `ort`'s default feature set asks for API 31, and a 1.23 runtime
  refuses: *"The requested API version [31] is not available, only API versions
  [1, 23] are supported in this build"* — so every ONNX call failed, every time,
  on an otherwise correct install.
- A failed file's recorded reason now carries the whole cause chain. It was
  written with `to_string()`, which prints only the outermost `anyhow` context,
  so a real decode failure was recorded as `decoding <path>` and the actual
  cause — the HRESULT, the malformed header — was thrown away. The reason is
  the only diagnostic the user has (SECURITY.md: skips are recorded, never
  swallowed), so the cause now reaches it.
