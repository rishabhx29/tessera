# Tessera — Architecture

Crate layout, data model, and per-format pipelines for v0.1. This describes
the target; the engine lands in weeks 2–3 of the plan in
`docs/product-definition.md`.

## Repository layout

```
tessera/
├── crates/core/        the engine: watcher, pipelines, store, search, CLI
│   └── src/
│       ├── watch/        folder watching and initial scan (notify crate)
│       ├── index/        the Indexer trait + per-format pipelines
│       │   ├── image.rs    decode → OCR → CLIP embedding
│       │   ├── pdf.rs      text layer → pages → chunks → MiniLM
│       │   ├── docx.rs     XML extraction → chunks → MiniLM
│       │   └── text.rs     txt/md/csv/log → chunks → MiniLM
│       ├── models/      ONNX Runtime wrappers, checksum-pinned downloads
│       ├── store/       SQLite schema, migrations, FTS5 + sqlite-vec
│       └── search/      the arms and RRF fusion
├── crates/app/         Tauri shell: hotkey, window, tray, settings
└── docs/
```

`tessera-core` never depends on `tessera-app`. The shell calls core through a
small API: `add_folder`, `search(query, filters)`, `index_status()`.

## The Indexer trait (the seam that makes formats cheap)

```rust
trait Pipeline {
    fn accepts(&self, path: &Path) -> bool;
    fn process(&self, file: &FileRef) -> Result<Vec<Tile>, PipelineError>;
}
```

A **tile** is the unit of indexing: an image, or a document page/chunk. Every
pipeline emits tiles; the store and search code are format-agnostic. Video
(v0.2) is a new pipeline that emits image tiles (keyframes) and text tiles
(transcript chunks) — no store or search changes.

## Data model (single SQLite file, WAL mode)

```sql
files(
  id INTEGER PRIMARY KEY,
  path TEXT UNIQUE,          -- absolute, canonicalized
  kind TEXT,                 -- image | pdf | docx | text | (video in v0.2)
  size INTEGER,
  mtime INTEGER,
  sha1 TEXT,                 -- change detection
  status TEXT,               -- indexed | skipped | failed | awaiting_ocr
  skip_reason TEXT,          -- 'too_large' | 'cloud_placeholder' | 'no_text_layer' | ...
  indexed_at INTEGER,
  thumb TEXT                 -- thumbnail path in the cache dir
);

tiles(
  id INTEGER PRIMARY KEY,
  file_id INTEGER REFERENCES files(id),
  seq INTEGER,               -- page/position within the file
  text TEXT                  -- OCR text or document chunk text
);

tiles_fts  -- FTS5 external-content over tiles, trigram tokenizer
img_vec(   -- sqlite-vec virtual table, 512-d CLIP, one row per image file
  file_id INTEGER PRIMARY KEY,
  embedding float[512]
);
txt_vec(   -- sqlite-vec virtual table, 384-d MiniLM, one row per text tile
  tile_id INTEGER PRIMARY KEY,
  embedding float[384]
);
meta(
  key TEXT PRIMARY KEY,      -- schema_version, clip_model, minilm_model, ocr_model ...
  value TEXT
);
```

Rules that exist because someone already got burned:

- **`meta` pins every model version.** Changing the CLIP or MiniLM model
  invalidates the corresponding vectors and forces re-indexing. Never mix
  vector spaces — a silent mix looks exactly like "search got worse for no
  reason".
- **One SQLite file, no second store.** Vectors, text, and metadata live
  together or drift apart.
- **SKIP reasons are recorded, not swallowed.** "Search can't find my scanned
  invoice" must be answerable from the diagnostics page.

## Indexing flow

```
watcher event / initial scan (newest-first)
        │
        ▼
  change detection (mtime + size + sha1)   ── unchanged → skip
        │
        ▼
  cloud placeholder?  ── yes → status='skipped', reason='cloud_placeholder'
        │ no
        ▼
  size > 50 MB?      ── yes → status='skipped', reason='too_large'
        │ no
        ▼
  pipeline.process(file)  → tiles { text, embeddings }
        │
        ▼
  store upsert (files, tiles, tiles_fts, img_vec, txt_vec) — one transaction
```

Queue policy: newest-first, up to 4 workers, models loaded per batch and
unloaded after; the whole pipeline pauses on battery / per the user's schedule.

## Search flow

```
query
 ├─ lexical arm   → FTS5 trigram BM25 over tiles_fts        (filenames + OCR + doc text)
 ├─ image arm     → CLIP text query → img_vec cosine          (description queries)
 ├─ text arm      → MiniLM query   → txt_vec cosine             (meaning over documents)
 └─ filters       → type / folder / date
        │
        ▼
 Reciprocal Rank Fusion (rank-based; score scales are incomparable)
        │
        ▼
 ranked tiles, each annotated with the arm(s) that surfaced it
```

One search box. The user does not choose modes; the arms all run and the
fusion hides which ones hit. Typo tolerance comes from the trigram tokenizer;
`inv0ice` finds `invoice`.

## Models (week-1 spike decides versions)

| Model | Purpose | Notes |
|---|---|---|
| RapidOCR PP-OCRv5 (ONNX, mobile) | OCR for images/screenshots | ~15 MB; multilingual models exist (Devanagari is a v0.1.x fast-follow) |
| MobileCLIP2-S0 or SigLIP-base (int8 ONNX) | image↔text description search | ~60–100 MB |
| all-MiniLM-L6-v2 (int8 ONNX) | document meaning search | ~90 MB |
| *(fallback)* Windows.Media.Ocr | OCR, Windows-only | nominated only if the spike shows RapidOCR too slow on CPU |

All downloaded once on first run, checksum-pinned (ADR-0004). No network
access at query time, ever.

## Deliberate non-decisions (until their phase)

- Video decode and whisper transcription: v0.2, as new pipelines.
- MCP transport: v0.3, over the same search API.
- Auto-organize: v0.4, dry-run first; it is the only surface that writes to
  user files and gets an ADR of its own.
