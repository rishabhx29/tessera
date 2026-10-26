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
│       ├── vectors/     embeddings, cosine, and the SigLIP embedder
│       ├── store/       SQLite schema, migrations, FTS5 + the vector tables
│       └── search/      the arms and RRF fusion
├── crates/app/         Tauri shell: hotkey, window, tray, settings
└── docs/
```

`tessera-core` never depends on `tessera-app`. The shell calls core through a
small API: `add_folder`, `search(query, filters)`, `index_status()`.

## The Indexer trait (the seam that makes formats cheap)

```rust
trait Pipeline {
    /// The kind this pipeline produces for `path`, or None if it does not own
    /// it. One method, not two: "do you handle this?" and "what kind is it?"
    /// cannot usefully disagree.
    fn accepts(&self, path: &Path) -> Option<Kind>;
    fn process(&self, path: &Path) -> Result<Extraction>;
}
```

`Extraction` is `Tiles(Vec<Tile>)` or `AwaitingOcr`. The third outcome is not
an error: a scan has no text layer, the bytes were read successfully, and
nothing in this build can read words out of it — so the loop records the file
as awaiting the OCR pass, still pending work, rather than as a failure.

This seam is the repo's one deliberate speculative generality, licensed by the
locked spec's four v0.1 formats. It stays open for formats the spec names, not
for anything else.

A **tile** is the unit of indexing: a numbered chunk of a document, or the
whole extracted text of an image. It carries `seq`, `text`, and — for a paged
format — the 1-based `page` it came from, so a result can say "page 4" rather
than "chunk 17". Every pipeline emits tiles; the store and search code are
format-agnostic. Video (v0.2) is a new pipeline that emits image tiles
(keyframes) and text tiles (transcript chunks) — no store or search changes.

The engine's public surface is small on purpose. `index_folder(store, pipelines,
folder) -> IndexSummary` is the whole indexing interface; `Store` exposes file
lifecycle operations (`index_file`, `skip_file`, `fail_file`, `await_ocr`,
`move_file`, `moved_candidate`, `remove_missing`) and search reads
(`file_ids_matching`, `files_by_ids`). Two seams are deliberately *internal* rather than public,
because they have one adapter each and exist for testability: the file probe
(so a Files On-Demand placeholder can be tested) and the hash thunk (so the
change decision is a pure function).

Two rules keep multi-step facts from leaking out of the store as protocols a
caller has to reconstruct:

- **A move is one question.** `moved_candidate(root, sha, path)` decides
  whether these bytes are a row that used to live elsewhere, including whether
  the old path is really gone. The caller never inspects paths to find out.
- **Re-titling is the store's job.** `move_file` takes the new path and
  updates the filename at the head of the first tile itself, inside the same
  transaction as the path — so the row and its searchable text cannot disagree,
  and no caller has to know the filename convention.

### The OCR seam (cross-platform, ADR-0007)

OCR is a trait with several implementations, chosen per platform:

```rust
pub trait OcrEngine: Send + Sync {
    fn name(&self) -> &'static str;
    fn recognize(&self, path: &Path) -> Result<String>;
}
```

- **WindowsOcr** - Windows.Media.Ocr (`windows` crate). Measured p50 **34.4 ms**
  per screenshot on this machine's real corpus, extracting 188 characters.
- **VisionOcr** - macOS Vision `VNRecognizeTextRequest` (`objc2-vision`),
  cfg-gated to macOS. **Not yet measured** - it is exercised on a macOS CI
  runner against a committed fixture, so it is known to work, and nothing is
  known about how fast it is (`docs/performance.md`).
- **OnnxOcr** - PP-OCRv6 det-tiny + rec-small via `ort` (ONNX Runtime), the
  cross-platform fallback and the quality pass. Measured p50 **949.6 ms**
  extracting 288 characters - 28x slower and 53% more text, and it read all 40
  corpus images where the native engine read 39 (`docs/performance.md`).

The engine is handed a **path, not pixels**, and that is deliberate: Windows
decodes an image in a few milliseconds and understands every format the OS
renders, so decoding it ourselves first would add work and lose formats for
nothing. Decoding is therefore the engine's business, and the seam says only what
must come out - the text in the file. It is also why the two engines need
different decoders: the Windows engine asks the OS, and `OnnxOcr` uses the
`image` crate, because a library engine has no operating system to ask.

Selection order (`ocr::preferred_engine`): the platform engine whenever it
exists, then ONNX if its pinned models are installed and verified, then an engine
that explains itself per file. An engine the user *named* and did not get is an
error that stops the run; the platform default being absent is not, because the
text formats keep working and each image is recorded as failed with the reason.

The image pipeline depends on the trait, never on a platform API, so adding a
platform never touches the store, search, or UI. Under `index/`, only
`index/skip.rs` touches platform APIs (`std::os::windows` file attributes), and
that is about cloud placeholders rather than reading text. Under `ocr/`, the
files that name a platform API are exactly the adapters — `ocr/windows.rs` and
`ocr/vision.rs` — and nothing else. That boundary is enforced by
`the_image_pipeline_never_imports_a_platform_api` in `tests/images.rs`, which
walks `src/` rather than trusting a hand-written file list, and which fails if
an exemption is left behind for a file that no longer exists.

The pipeline (`index/image.rs`) calls the engine and emits **one tile per
image**: a screenshot is one unit of search, and splitting its text into
word-windows would split the words a person remembers seeing together.
Documents chunk; screenshots do not. An image with no text is indexed, not
failed — it still has a filename to be found by. A decode failure is recorded
against that one file with its reason, and the run continues.

### Two ONNX Runtime facts the code is shaped around

Both look fine until they fail in production, so they are properties of the
module rather than comments scattered through it:

- **One `Environment` per process.** `ort::init()` fails on the second call, so
  the environment is created once behind a `OnceLock` and shared. Without this,
  building a second engine — one per CLI command, one per index worker, one per
  test — fails outright.
- **Sessions are not safe for concurrent `run`.** The trait is `Send + Sync`, so
  sessions are handed out from a free list and created on demand, rather than one
  session behind a lock. A lock would be simpler and would also serialise the
  whole engine to one image at a time.

Everything the spike debugged is a named, tested function rather than a constant
with a comment: the alphabet (`['blank'] + lines + [' ']`, the trailing space
being the one `lines()` hides), greedy CTC decoding, the score gate, the 32-pixel
alignment the detector graph demands, and batching recognition by total width.

### The runtime dll is part of the build (packaging contract)

`ort`'s API level is pinned to 23 (the 1.23 line) in `Cargo.toml`, and on Windows
that pin is only as correct as the `onnxruntime.dll` the loader picks. Windows
searches the executable's own directory before `System32`, and `System32` ships
an `onnxruntime.dll` **1.17.1**, which does not answer API 23. A build whose 1.23
dll is not beside the executable therefore binds to the System32 copy and every
ONNX call dies.

The engine cannot fix that from the inside; it can only name it. `OnnxOcr`'s
init failure probes the loaded runtime — `OrtGetApiBase.GetVersionString` on the
module already mapped into the process — and reports the version alongside the
System32 hint, so the failure is a packaging bug someone can act on rather than
ort's bare panic. The packaging task is to bundle the 1.23 dll beside
`tessera.exe` and assert its presence in the packaging test; until that lands,
the failure names itself.

## Data model (single SQLite file, WAL mode)

```sql
files(
  id INTEGER PRIMARY KEY,
  path TEXT UNIQUE,          -- absolute, canonicalized
  root TEXT,                 -- the indexed folder this row belongs to
  kind TEXT,                 -- image | pdf | docx | text | (video in v0.2)
  size INTEGER,
  mtime INTEGER,             -- nanoseconds since the epoch (two writes in one
                              -- second are different files, not one)
  sha1 TEXT,                 -- content hash: change detection + move detection
  status TEXT,               -- indexed | skipped | failed | awaiting_ocr
  skip_reason TEXT,          -- 'too_large' | 'cloud_placeholder' | 'no_text_layer' | ...
  indexed_at INTEGER,        -- last outcome time; for a non-indexed row it is the
                             -- last *attempt*, which the retry deadline counts from
  attempts INTEGER,          -- consecutive non-indexed attempts; the retry backoff
                             -- is exponential in this, and an extraction clears it
  thumb TEXT                 -- thumbnail path in the cache dir
);

tiles(
  id INTEGER PRIMARY KEY,
  file_id INTEGER REFERENCES files(id),
  seq INTEGER,               -- 0-based position within the file
  page INTEGER,              -- 1-based page for paged formats, NULL otherwise
  text TEXT                  -- OCR text or document chunk text
);

tiles_fts  -- FTS5 external-content over tiles, trigram tokenizer
img_vec(   -- one row per image file: SigLIP-base is 768-d
  file_id INTEGER PRIMARY KEY,
  dim     INTEGER NOT NULL,
  vec     BLOB NOT NULL     -- little-endian f32, unit length
);
txt_vec(   -- one row per text tile: MiniLM is 384-d
  tile_id INTEGER PRIMARY KEY,
  dim     INTEGER NOT NULL,
  vec     BLOB NOT NULL
);
meta(
  key TEXT PRIMARY KEY,      -- schema_version, vectors_image_version, ...
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
- **Cheap evidence before content.** `size` + `mtime` decide "unchanged" with
  a stat; the sha1 is computed only when they disagree — including when the
  timestamp moved *backwards* (a restored archive) or the size held still
  (`port=8080` → `port=9090`). Re-indexing a 20,000-file corpus must not read
  20,000 files.
- **A failed attempt is not retried forever, and not on every walk.** A row that
  was never extracted (`skipped`, `failed`, `awaiting_ocr`) is work owed, but the
  watcher admits it only when the file changed or its backoff — five minutes,
  doubling to an hour, driven by `attempts` — has passed. `tessera index` is an
  explicit request and still retries every time; only the watcher's admission is
  gated. Without the gate a four-file corpus with one unreadable PDF re-extracted
  it on every 60 s rescan for the life of the process.
- **A row's `root` scopes deletion.** After a walk, rows under that root whose
  file is gone from disk are dropped, so a deletion propagates — and indexing
  one folder can never forget another. Existence is checked directly: a folder
  the user cannot read looks exactly like an empty one.
- **A skipped file is never opened.** Cloud placeholders and oversize files are
  decided from metadata alone, before anything reads a byte.
- **A hash is a claim about content we read.** A skipped or failed row carries
  none, which also stops a stale hash from dragging a broken row to a new path.

## Indexing flow

```
watcher event / initial scan (newest-first)
        │
        ▼
  change detection (mtime + size, then sha1)   ── unchanged → verify only
        │
        ▼
  cloud placeholder?     ── yes → status='skipped', reason='cloud_placeholder'
        │ no                                   (and its bytes are never fetched)
        ▼
  size > 50 MB?          ── yes → status='skipped', reason='too_large'
        │ no
        ▼
  pipeline.process(file)  → tiles { seq, page, text } | AwaitingOcr (no tiles,
        │                     status='awaiting_ocr', retried next run)
        │
        ▼
  store upsert (files, tiles, tiles_fts) — one transaction
        │
        ▼
  arms write their vectors after it: img_vec per file from the image pipeline,
  txt_vec by a post-walk sweep over tiles that have no vector yet (a moved pin,
  a fresh install and a crash all repair through that same sweep)
        │
        ▼
  rows under this root whose file is gone → deleted (deletion propagates)
```

Queue policy: newest-first, up to 4 workers, models loaded per batch and
unloaded after; the whole pipeline pauses on battery / per the user's schedule.

## Watch mode

`tessera watch` is the same per-file pass `tessera index` runs
(`index::process_one`), fed by a different source of truth. One coordinator owns
the queue, the registrations, the power poll and the per-drain duties; up to four
workers each own a `Store` connection and do the extraction. The backlog is a
max-heap on mtime, so a file saved mid-backlog outranks the backlog, and one live
entry per path keeps a save's several events from becoming several extractions.

notify is the trigger, never the record. Every event re-stats the path, a notify
overflow (which drops events by design) is treated as a full rescan, and each
root is re-walked every 60 s regardless — a missed event costs at most one
interval, which is what makes the mode work on network shares too.

The coordinator's own store calls can fail while the store is shared with another
`tessera index`, a backup tool or a WAL checkpoint. A failure is reported, a
rescan is owed, and the call is retried on a backoff that doubles from one second
to a minute; twenty consecutive failures stop the run, because that is not
transient. While it waits, the coordinator sleeps on the event feed for the rest
of the backoff, capped by the one-second shutdown tick — *not* for the "nearest
timer" it uses when working. That answer reports an already due timer (the power
poll, a rescan) as zero, and the waiting pass runs neither, so a zero there is an
immediate return: a spin rather than a wait. An event delivered during the outage
is read and dropped on purpose — nothing can be recorded, and the rescan every
failure owes is what finds whatever it announced.

Three platform facts sit behind traits, because each is a per-OS answer with no
portable API and none of them can be conjured up in a test: the event feed, the
power source and the process priority. On Windows the process runs in
`IDLE_PRIORITY_CLASS` with its workers at the lowest thread priority, and the
power state is `GetSystemPowerStatus` polled twice a minute. **On other platforms
the priority is unchanged and the power state is `Unknown`**, which the default
schedule treats as "not on battery": refusing to index because the platform
cannot report a battery would be a worse failure than running when it might be
on one.

## Search flow

```
query
 ├─ lexical arm   → FTS5 trigram BM25 over tiles_fts        (filenames + OCR + doc text)
 ├─ image arm     → SigLIP text query → img_vec cosine        (description queries)
 ├─ text arm      → MiniLM query   → txt_vec cosine             (meaning over documents)
 └─ filters       → type / folder / date, applied inside every arm
        │
        ▼
 weighted Reciprocal Rank Fusion (rank-based; score scales are incomparable)
        │
        ▼
 ranked files, at the matching tile, each annotated with the arm(s) that
 surfaced it
```

One search box. The user does not choose modes: the three arms run on
threads, one store connection each (WAL lets them read together), and
`fusion::fuse` merges their lists with each arm's vote scaled by how much
text the file carries — the measured answer to what an equal image voice
costs on text-bearing files (0.964 → 0.375 top-1; ADR-0003's amendment holds
the table). `--describe` and `--describe-text` remain as arm-isolating
diagnostics, not modes. The text arm embeds every tile the store
holds, so a document chunk, an OCR line and a file name are all findable by
meaning. Typo tolerance has two layers: the trigram
tokenizer makes partial words match (`invoic` finds `invoice`), and each query
term is expanded into its lookalike variants (`0`↔`o`, `1`↔`l`↔`i`, `5`↔`s`,
`8`↔`b`) and OR-ed, so `inv0ice` finds `invoice`. Trigram alone cannot do this
— a typo is not a substring. A second *folded* index (write-time lookalike
folding, gyotaku's mechanism) is the planned way to also survive OCR misreads
inside the corpus.

## Models (week-1 spike decides versions)

| Model | Purpose | Notes |
|---|---|---|
| RapidOCR PP-OCRv5 (ONNX, mobile) | OCR for images/screenshots | ~15 MB; multilingual models exist (Devanagari is a v0.1.x fast-follow) |
| MobileCLIP2-S0 or SigLIP-base (int8 ONNX) | image↔text description search | ~60–100 MB |
| all-MiniLM-L6-v2 (int8 ONNX) | document meaning search | ~24 MB (the pinned int8 set); one vector per tile, documents and OCR text alike |
| *(fallback)* Windows.Media.Ocr | OCR, Windows-only | nominated only if the spike shows RapidOCR too slow on CPU |

All downloaded once on first run, checksum-pinned (ADR-0004). No network
access at query time, ever.

## Model delivery

`models::Models` owns first-run delivery. The pins live in
`crates/core/src/models/lock.json`, embedded in the binary with `include_str!`:
a checksum that can be edited at runtime is not a pin. The file is derived from
`spike/model-lock.json` (the spike's measured record) and keeps only the models
v0.1 runs.

```
<data dir>/models/
├── siglip-vision.int8.onnx      image arm   (arm: image)
├── siglip-text.int8.onnx
├── siglip-tokenizer.json
├── siglip-preprocessor.json
├── minilm.int8.onnx             text arm    (arm: text)
├── minilm-tokenizer.json
├── ppocrv6-det-tiny.onnx        OCR         (no arm: produces text, not vectors)
└── ppocrv6-rec-small.onnx
```

Rules the module enforces, each with a test:

- **Nothing runs unverified.** A download streams to a temporary name while
  being hashed; it is renamed into place only after it matches its pin, and is
  deleted otherwise. A file that is present but wrong is a loud error, never a
  silent re-use.
- **One network access, ever.** `ensure()` hashes what is already there and
  only fetches what is missing, so a warm run touches no network. Verification
  happens on the indexing path, never on the query path — a keyword search does
  not read the models directory at all.
- **A native OS engine downloads nothing.** Consumers ask for the sets they
  need by name, so Windows.Media.Ocr and macOS Vision pass an empty list. The
  set names and the `--models-dir` override exist for that.
- **A changed pin invalidates its own arm, and only that arm.** The version
  token is a digest over a set's pins (sorted, so re-ordering the manifest
  cannot invalidate anyone's vectors), recorded in `meta` beside the vectors it
  describes (`vectors_image_version`, `vectors_text_version`). A mismatch makes
  the arm outstanding work; it is never resolved by blending old vectors with
  new ones.
- **Every source is HTTPS.** Loopback is the one exception, because a model
  fetched from `127.0.0.1` never crosses a network — and it is what lets the
  download path be tested at all against a local server. A test asserts every
  pinned source is HTTPS.

`cargo test --test models -- --ignored` re-verifies one pinned checksum against
upstream, so the pins can be proved rather than trusted forever.

## Deliberate non-decisions (until their phase)

- Video decode and whisper transcription: v0.2, as new pipelines.
- MCP transport: v0.3, over the same search API.
- Auto-organize: v0.4, dry-run first; it is the only surface that writes to
  user files and gets an ADR of its own.
