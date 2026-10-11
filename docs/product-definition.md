# Tessera — Product Definition v1.0

- Status: **Locked** (2026-10-11). Superseding this requires an ADR.
- Source of truth: decisions consolidated from four research rounds
  (competitor survey, GitHub evidence, demand signals, positioning tests).

---

## 1. What Tessera is

**One search bar for everything your PC can't search — images, screenshots,
PDFs, and documents — offline, free, and open source. The same index is
exposed to AI agents via MCP (phase 3).**

### Positioning (the argument, per audience)

- **vs the commercial tools** (FileScope $19, Fossick ~$15, PixFinder,
  MANY Finder, Filect $15/mo): same capability, free, open, no account — plus
  mixed-format results and an agent surface they cannot copy.
- **vs Microsoft**: the Copilot+ semantic-search experience on the PC you
  already own, across every folder, including formats Microsoft's indexing
  does not touch.
- **vs OSS neighbors** (gyotaku — screenshots only; FrameScout — images+video
  with no positioning; rememex — file search): the one that indexes
  *everything* rather than one format.

### What Tessera is not

Not a screen recorder. Not a photo manager. Not cloud software. Not paid.
No account, no telemetry, no network calls at query time.

---

## 2. The v0.1 demo (must land in 10 seconds)

The user presses `Alt+Space`, types **"the invoice from the airline"**, and one
results grid returns:

- the **screenshot** (OCR text, matched words highlighted),
- the **PDF page** (text snippet, page number),
- the **docx** (snippet).

Three formats, one query — the result no competitor produces today. Second
half of the demo: a freshly saved file becomes searchable within seconds,
visible as live indexing progress.

---

## 3. v0.1 feature spec (locked)

### 3.1 Search — one box, hybrid by default

- **Lexical arm**: SQLite FTS5 with the trigram tokenizer for typo tolerance
  (`inv0ice` → `invoice`), BM25 ranking. Matches OCR text, document text,
  filenames.
- **Image arm**: CLIP text↔image embedding space for description queries
  ("the one with the red chart").
- **Text arm**: MiniLM embeddings over document chunks for meaning-based
  document search.
- **Fusion**: up to three arms fused with Reciprocal Rank Fusion (rank-based,
  not score-based — BM25 and cosine scores are not comparable).
- **Filters** (minimal): `type:image|pdf|doc`, `in:folder`, `before:`/`after:`.
- **Result honesty**: every hit shows which arm surfaced it and why
  (highlighted words, match strength).

### 3.2 Formats

| Format | Pipeline |
|---|---|
| png / jpg / jpeg / webp / bmp / gif / tiff | OCR (RapidOCR PP-OCRv5, ONNX) + CLIP embedding |
| pdf (text layer) | page text extraction + chunking + MiniLM |
| docx | XML text extraction (document.xml) + chunking + MiniLM |
| txt / md / csv / log | plain text + chunking + MiniLM |

Out of v0.1, on purpose: **video** (v0.2), **xlsx/pptx** (v0.4),
**scanned-PDF OCR** (v0.1.x, opt-in). PDFs without a text layer are flagged
and counted in the diagnostics page — never silently skipped.

### 3.3 Indexing policy

- **Newest-first** initial scan: a fresh screenshot is searchable in seconds,
  even while a 20,000-file backlog processes.
- Idle CPU/IO priority; pause on battery; user-configurable schedule
  (idle-only / always / paused).
- Models load per batch and unload after; hard RAM ceiling (~250 MB idle).
  On constrained machines the engine queues work rather than degrading search.
- **OneDrive online-only placeholders are skipped, never hydrated** — indexing
  a cloud-backed Pictures folder must not download the cloud.
- Caps: files >50 MB skipped (counted, shown in diagnostics); changes detected
  via mtime+size+sha1 so renames and moves update the index, not duplicate it.
- Concurrency: up to 4 workers, newest-first queue priority.
- Diagnostics page: indexed / skipped (with reason) / failed / scanned-PDFs
  awaiting OCR.

### 3.4 UI (Tauri + React)

- Global hotkey (`Alt+Space` default) summoning a centered search window.
- Results grid: thumbnails for images, type badges for documents, folder,
  match reason.
- Preview pane: image → OCR text with matches highlighted, one-click copy;
  document → snippet around the match with page number, open / reveal-in-folder.
- Keyboard-first navigation (arrows, Enter, Esc).
- Tray icon + launch-at-login; settings for folders, exclusions, schedule,
  shortcut, models, language packs.

### 3.5 Privacy posture

Everything runs locally. No telemetry, no accounts, no update pings. Models
download once on first run, checksum-pinned. The index is a single SQLite file
the user can inspect, move, or delete.

---

## 4. Phase plan (each phase is a relaunch on the same index)

| Phase | Surface | The demo |
|---|---|---|
| **v0.1** | Images (OCR+CLIP) + documents (text layer) | One query, mixed-format results. Windows-first |
| **v0.2** | Video files (keyframes + CLIP; Whisper for audio) | Search inside recorded meetings and calls — no consumer tool owns this |
| **v0.3** | Phone camera roll (auto-detect `OneDrive\Pictures\Camera Roll`) + MCP server | Phone screenshots searchable on your PC (mainstream); index queryable by Claude/Cursor (developers) |
| **v0.4** | Auto-organize (suggested names/filing, dry-run + opt-in) + xlsx/pptx + more languages | The folder that tidies itself |

---

## 5. Build plan (10–20 hrs/week → 9–10 weeks to launch)

| Week | Milestone | Exit criteria |
|---|---|---|
| 1 | **Spike + scaffold**: measure RapidOCR + CLIP + MiniLM over 1,000 real images and 200 real PDFs/DOCX | Real numbers meet targets, or model swap decided (fallback: Windows.Media.Ocr). **If OCR > 1 s/image, stop and rethink — do not build on hope** |
| 2–3 | Engine: watcher, queue, change detection, per-format pipelines, store schema, hybrid search, CLI | `tessera search "invoice airline"` returns the right screenshot *and* the right PDF from 5k mixed items |
| 4–5 | UI: hotkey window, results grid, preview with highlighted matches, filters, tray, settings, indexing progress | Daily use replaces Explorer search |
| 6 | Packaging: GitHub Actions releases, one-line install + setup.exe, autostart, uninstall, scoop manifest | A friend on a clean PC installs it and finds a file without asking for help |
| 7 | Dogfood, measure everything (README numbers page), docs site, community files | Every README number has methodology behind it |
| 8–9 | **Launch**: demo video (mixed-format query), Show HN, X, Product Hunt; then releases every 2 weeks | — |

---

## 6. Performance targets (measured in week 1, published only when real)

| Metric | Target | Why it matters |
|---|---|---|
| Query end-to-end @ 50k items | < 100 ms | "Instant" is the product; lexical alone should be 1–10 ms/keystroke |
| Image saved → searchable | < 1.5 s | The demo's second half |
| Document saved → searchable | < 300 ms | Backlog feel |
| Idle RAM | < 250 MB | Old hardware is the beachhead |
| Disk per 1,000 items | ~25 MB | Users won't index 50 GB of photos twice |

See `docs/performance.md` for measured results once the spike runs.

---

## 7. Risk register

1. **SEO/content war** with commercial incumbents — conceded on Google; the play
   is HN / Reddit / YouTube / X and the "open source alternative" search
   corridor.
2. **The unified-everything graveyard** (aperture: 0 stars, rememex: 68 then
   dead, FrameScout: 8) — those shipped platform feature-lists with no demo.
   The defense is the day-one mixed-format demo plus measured performance.
   Craft is the only moat this category offers.
3. **gyotaku ships description-search** — no longer existential; the wedge was
   never screenshots-only.
4. **Model cost on old hardware** — mitigated by design (idle-only, RAM
   ceilings, queueing) and falsified-or-confirmed by the week-1 spike.
5. **Maintainer burnout** — the real killer of solo free tools. Policy: ≤30
   min/week triage, releases every 2 weeks (not more).

---

## 8. Kill criteria (pre-committed; see ADR-0005)

Eight weeks after launch: **< 300 stars or < 1,000 downloads → do not build
v0.2–0.4.** Pivot to video-only (the emptiest lane) or stop the project.
Deciding this now is what prevents a 68-star abandonware repo.

## 9. Success definition (beyond vanity stars)

- Week 8 post-launch: 300+ stars, 1,000+ release downloads.
- Month 6: 2k+ stars, 30+ issues from strangers, 3+ external contributors,
  one press/newsletter mention, MCP listed in a registry. That cluster is
  "legitimate" in a way stars alone do not prove.
