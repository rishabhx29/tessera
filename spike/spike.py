"""Tessera week-1 spike — measure before building.

One question: do the product's chosen models and store meet the latency
targets in docs/product-definition.md on *this* machine with *real* files?

Measures, per docs/performance.md's methodology:
  1. per-stage cost: decode / OCR / CLIP-embed (images), extract / chunk /
     MiniLM-embed (documents)
  2. save -> searchable end-to-end for one image and one document
  3. fused query latency (lexical + image arm + text arm, RRF) at the real
     corpus size, then at 1k / 10k / 50k via replication (labelled as
     estimates — never presented as measurements)
  4. cross-modal sanity: can text(OCR of an image) retrieve that image?
     A near-random rank means the embedding space is wrong and the spike
     fails even if everything is fast
  5. hardware, package versions, model hashes

Privacy: nothing leaves this machine. results.json contains extracted text
from your files; results.md contains counts and timings only.
"""

from __future__ import annotations

import argparse
import ctypes
import json
import platform
import statistics
import sys
import time
from pathlib import Path

import numpy as np

ROOT = Path(__file__).parent
RESULTS = ROOT / "results"
RESULTS.mkdir(exist_ok=True)

IMAGE_EXTS = {".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".tiff"}
DOC_EXTS = {".pdf", ".docx", ".txt", ".md", ".csv", ".log"}
DEFAULT_ROOTS = [
    Path.home() / "Pictures",
    Path.home() / "OneDrive" / "Pictures" / "Camera Roll",
    Path.home() / "Downloads",
    Path.home() / "Desktop",
    Path.home() / "OneDrive" / "Documents",
    Path.home() / "Documents",
]

FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS = 0x40000
FILE_ATTRIBUTE_RECALL_ON_OPEN = 0x100000


def is_cloud_placeholder(path: Path) -> bool:
    """OneDrive 'online-only' files exist locally as metadata only."""
    try:
        attrs = ctypes.windll.kernel32.GetFileAttributesW(str(path))
    except Exception:
        return False
    if attrs == 0xFFFFFFFF:
        return False
    return bool(attrs & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN))


def discover(exts, limit, include_cloud):
    files, skipped_cloud = [], 0
    seen = set()
    for root in DEFAULT_ROOTS:
        if not root.exists():
            continue
        try:
            walker = root.rglob("*")
        except OSError:
            continue
        for p in walker:
            if len(files) >= limit:
                break
            try:
                if not p.is_file() or p.suffix.lower() not in exts:
                    continue
                rp = p.resolve()
                if rp in seen:
                    continue
                if is_cloud_placeholder(p) and not include_cloud:
                    skipped_cloud += 1
                    continue
                seen.add(rp)
                files.append((p, p.stat().st_mtime))
            except OSError:
                continue
    files.sort(key=lambda t: t[1], reverse=True)  # newest first, like the engine
    return [f for f, _ in files], skipped_cloud


def import_sqlite():
    """Stock python.org sqlite3 on Windows refuses load_extension ('not
    authorized'); sqlite-vec needs it. APSW allows it and bundles SQLite 3.53
    (FTS5 + trigram verified)."""
    import apsw

    return apsw


def pct(xs, p):
    if not xs:
        return None
    return round(float(np.percentile(xs, p)), 1)


def mean(xs):
    return round(float(np.mean(xs)), 1) if len(xs) else None


def chunk_text(text, words=200, overlap=50):
    ws = text.split()
    if not ws:
        return []
    step = max(1, words - overlap)
    return [" ".join(ws[i : i + words]) for i in range(0, len(ws), step)][:20]


def extract_doc_text(path: Path) -> str:
    suf = path.suffix.lower()
    try:
        if suf == ".pdf":
            from pypdf import PdfReader

            reader = PdfReader(str(path))
            return "\n".join((pg.extract_text() or "") for pg in reader.pages)
        if suf == ".docx":
            import docx

            d = docx.Document(str(path))
            return "\n".join(par.text for par in d.paragraphs)
        return path.read_text(errors="replace")
    except Exception as e:  # noqa: BLE001
        print(f"  (extraction failed for {path.name}: {e})", file=sys.stderr)
        return ""


def build_store(store_path):
    """Returns (conn, fts_tokenizer). Vec tables are fatal on failure;
    trigram FTS falls back to unicode61 on older SQLite builds."""
    sqlite3 = import_sqlite()
    import sqlite_vec

    if store_path.exists():
        store_path.unlink()
    conn = sqlite3.Connection(str(store_path))
    conn.execute("PRAGMA journal_mode=WAL")
    conn.enable_load_extension(True)
    conn.load_extension(sqlite_vec.loadable_path())
    conn.execute(
        "CREATE TABLE tiles(id INTEGER PRIMARY KEY, file_id INTEGER, kind TEXT, text TEXT)"
    )
    conn.execute(
        "CREATE VIRTUAL TABLE img_vec USING vec0(file_id INTEGER PRIMARY KEY, embedding float[768])"
    )
    conn.execute(
        "CREATE VIRTUAL TABLE txt_vec USING vec0(tile_id INTEGER PRIMARY KEY, embedding float[384])"
    )
    fts_tokenizer = "trigram"
    try:
        conn.execute("CREATE VIRTUAL TABLE tiles_fts USING fts5(text, tokenize='trigram')")
    except Exception:
        fts_tokenizer = "unicode61"
        conn.execute("CREATE VIRTUAL TABLE tiles_fts USING fts5(text, tokenize='unicode61')")
    return conn, fts_tokenizer


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--images", type=int, default=100)
    ap.add_argument("--docs", type=int, default=40)
    ap.add_argument("--include-cloud", action="store_true", help="also index OneDrive placeholders (hydrates them)")
    ap.add_argument("--query-runs", type=int, default=10)
    ap.add_argument("--scales", type=int, nargs="*", default=[1000, 10000, 50000])
    args = ap.parse_args()

    import psutil
    from PIL import Image

    proc = psutil.Process()
    sys.path.insert(0, str(ROOT))
    from models import MiniLM, OCR, SigLIP, ensure_models

    print("== models ==")
    t0 = time.perf_counter()
    lock = ensure_models()
    print(f"models ready in {time.perf_counter() - t0:.1f}s")

    print("== corpus discovery ==")
    images, cloud_skipped = discover(IMAGE_EXTS, args.images, args.include_cloud)
    docs, cloud_skipped_docs = discover(DOC_EXTS, args.docs, args.include_cloud)
    print(f"images: {len(images)} (cloud placeholders skipped: {cloud_skipped})")
    print(f"docs:   {len(docs)} (cloud placeholders skipped: {cloud_skipped_docs})")
    if not images and not docs:
        raise SystemExit("no corpus found — pass --roots or point --images at a folder")

    siglip, minilm, ocr = SigLIP(lock), MiniLM(lock), OCR()
    rss_models_loaded = proc.memory_info().rss / 1e6

    conn, fts_tokenizer = build_store(RESULTS / "spike.sqlite")
    print(f"store ready (FTS5 {fts_tokenizer})")

    # ---------------- image pipeline ----------------
    print("== image pipeline ==")
    t_decode, t_ocr, t_clip, t_e2e = [], [], [], []
    ocr_texts, img_ids = {}, []
    queries_lex, queries_desc = [], []
    # NOTE: stage timers are in MILLISECONDS (the first spike run recorded
    # seconds and printed them as ms; the data was right, the label was wrong).
    for i, path in enumerate(images):
        te = time.perf_counter()
        try:
            img = Image.open(path)
            img.load()
        except Exception:
            continue
        t_decode.append((time.perf_counter() - te) * 1000)
        t1 = time.perf_counter()
        text = ocr.text(img)
        t_ocr.append((time.perf_counter() - t1) * 1000)
        t2 = time.perf_counter()
        emb = siglip.embed_images([img])
        t_clip.append((time.perf_counter() - t2) * 1000)
        t_e2e.append((time.perf_counter() - te) * 1000)
        ocr_texts[i] = text
        img_ids.append(i)
        conn.execute("INSERT INTO tiles(id, file_id, kind, text) VALUES (?,?,?,?)", (i, i, "image", path.name + "\n" + text))
        conn.execute("INSERT INTO tiles_fts(rowid, text) VALUES (?,?)", (i, path.name + "\n" + text))
        conn.execute("INSERT INTO img_vec(file_id, embedding) VALUES (?,?)", (i, emb[0].astype(np.float32).tobytes()))
        # queries that actually have answers, mined from the corpus itself
        words = [w for w in text.split() if len(w) >= 5 and w.isalpha()]
        if words and len(queries_lex) < 20:
            queries_lex.append(words[0])
        if text.strip() and len(queries_desc) < 20 and i % 3 == 0:
            queries_desc.append(" ".join(text.split()[:12]))

    # ---------------- doc pipeline ----------------
    print("== doc pipeline ==")
    t_extract, t_chunk, t_embed = [], [], []
    tile_id = 100_000
    for j, path in enumerate(docs):
        t0d = time.perf_counter()
        text = extract_doc_text(path)
        t1d = time.perf_counter()
        chunks = chunk_text(text)
        t2d = time.perf_counter()
        for ch in chunks:
            emb = minilm.embed_texts([ch])
            conn.execute(
                "INSERT INTO tiles(id, file_id, kind, text) VALUES (?,?,?,?)",
                (tile_id, 10_000 + j, "doc", ch),
            )
            conn.execute("INSERT INTO tiles_fts(rowid, text) VALUES (?,?)", (tile_id, ch))
            conn.execute(
                "INSERT INTO txt_vec(tile_id, embedding) VALUES (?,?)",
                (tile_id, emb[0].astype(np.float32).tobytes()),
            )
            tile_id += 1
        t_extract.append((time.perf_counter() - t0d) * 1000)
        t_chunk.append((time.perf_counter() - t1d) * 1000)
        t_embed.append((time.perf_counter() - t2d) * 1000)

    rss_after_index = proc.memory_info().rss / 1e6

    # ---------------- cross-modal sanity ----------------
    print("== cross-modal sanity ==")
    sanity = {}
    best_text_variant = "pooler"
    sanity_ids = img_ids[: min(40, len(img_ids))]
    if len(sanity_ids) >= 8:
        img_embs = np.vstack(
            [siglip.embed_images([Image.open(images[i])]) for i in sanity_ids]
        )
        texts = [ocr_texts[i][:64] for i in sanity_ids]
        variants = siglip.embed_texts(texts)
        for name, t_embs in variants.items():
            sims = img_embs @ t_embs.T
            ranks = [int((sims[:, r] > sims[r, r]).sum() + 1) for r in range(len(sanity_ids))]
            sanity[name] = {
                "mean_rank_of_correct": mean(ranks),
                "median_rank_of_correct": float(np.median(ranks)),
                "n": len(sanity_ids),
                "top1_rate": round(sum(1 for r in ranks if r == 1) / len(ranks), 3),
            }
            print(f"  text-embedding variant '{name}': mean rank {sanity[name]['mean_rank_of_correct']} of {len(sanity_ids)}")
    if sanity:
        best_text_variant = min(
            sanity, key=lambda v: (sanity[v]["mean_rank_of_correct"], -sanity[v]["top1_rate"])
        )
        print(f"  --> using text embedding variant '{best_text_variant}' (best sanity score)")

    # ---------------- query benchmarks ----------------
    print("== query benchmarks (real corpus) ==")
    n_tiles = conn.execute("SELECT COUNT(*) FROM tiles").fetchone()[0]

    def bench(label, fn, runs=None):
        runs = runs or args.query_runs
        times, results = [], None
        for _ in range(runs):
            t = time.perf_counter()
            results = fn()
            times.append((time.perf_counter() - t) * 1000)
        return {
            "label": label,
            "p50_ms": pct(times, 50),
            "p95_ms": pct(times, 95),
            "hits": len(results or []),
            "sample_first": (results or [None])[0],
        }

    lexical_q = queries_lex[0] if queries_lex else "the"
    img_q_text = queries_desc[0] if queries_desc else "a screenshot"
    doc_q = " ".join(queries_desc[0].split()[:6]) if queries_desc else "document"
    timings = []

    def q_lexical():
        return [r[0] for r in conn.execute("SELECT rowid FROM tiles_fts WHERE tiles_fts MATCH ? LIMIT 20", (lexical_q,))]

    def q_image():
        emb = siglip.embed_texts([img_q_text])
        return [r[0] for r in conn.execute("SELECT file_id, distance FROM img_vec WHERE embedding MATCH ? AND k = 20", (emb[best_text_variant][0].astype(np.float32).tobytes(),))]

    def q_text():
        emb = minilm.embed_texts([doc_q])
        return [r[0] for r in conn.execute("SELECT tile_id, distance FROM txt_vec WHERE embedding MATCH ? AND k = 20", (emb[0].astype(np.float32).tobytes(),))]

    timings.append(bench("lexical (FTS5)", q_lexical))
    timings.append(bench("image arm (SigLIP text->vec)", q_image))
    timings.append(bench("text arm (MiniLM)", q_text))

    def q_fused():
        import math

        def rrf(lists, k=60):
            scores = {}
            for lst in lists:
                for rank, item in enumerate(lst, 1):
                    scores[item] = scores.get(item, 0.0) + 1.0 / (k + rank)
            return sorted(scores, key=scores.get, reverse=True)[:20]

        lex = q_lexical()
        emb_variants = siglip.embed_texts([img_q_text])
        img = [r[0] for r in conn.execute("SELECT file_id, distance FROM img_vec WHERE embedding MATCH ? AND k = 20", (emb_variants[best_text_variant][0].astype(np.float32).tobytes(),))]
        return rrf([lex, img, q_text()])

    timings.append(bench("fused (RRF of three arms)", q_fused))
    for t in timings:
        print(f"  {t['label']:<35} p50 {t['p50_ms']}ms  p95 {t['p95_ms']}ms  hits {t['hits']}")

    # ---------------- scale estimate (replication, clearly labelled) ----------------
    print("== scale estimates (replication) ==")
    import sqlite_vec

    sqlite3 = import_sqlite()

    scale_results = []
    img_blob = conn.execute("SELECT embedding FROM img_vec LIMIT 1").fetchone()[0]
    txt_blob = conn.execute("SELECT embedding FROM txt_vec LIMIT 1").fetchone()
    txt_blob = txt_blob[0] if txt_blob else None
    for target in args.scales:
        est = RESULTS / f"scale-{target}.sqlite"
        if est.exists():
            est.unlink()
        sconn = sqlite3.Connection(str(est))
        sconn.enable_load_extension(True)
        sconn.load_extension(sqlite_vec.loadable_path())
        sconn.execute(
            "CREATE TABLE tiles(id INTEGER PRIMARY KEY, file_id INTEGER, kind TEXT, text TEXT)"
        )
        sconn.execute(
            f"CREATE VIRTUAL TABLE tiles_fts USING fts5(text, tokenize='{fts_tokenizer}')"
        )
        sconn.execute(
            "CREATE VIRTUAL TABLE img_vec USING vec0(file_id INTEGER PRIMARY KEY, embedding float[768])"
        )
        sconn.execute(
            "CREATE VIRTUAL TABLE txt_vec USING vec0(tile_id INTEGER PRIMARY KEY, embedding float[384])"
        )
        blocks = max(1, target // max(1, len(img_ids)))
        tfill = time.perf_counter()
        sconn.execute("BEGIN")
        for b in range(blocks):
            rows, fts_rows, vec_rows = [], [], []
            for i in range(len(img_ids)):
                fid = b * len(img_ids) + i
                rows.append((fid, fid, "image", "replicated tile"))
                fts_rows.append((fid, "replicated tile"))
                vec_rows.append((fid, img_blob))
            sconn.executemany("INSERT INTO tiles VALUES (?,?,?,?)", rows)
            sconn.executemany("INSERT INTO tiles_fts(rowid,text) VALUES (?,?)", fts_rows)
            sconn.executemany("INSERT INTO img_vec VALUES (?,?)", vec_rows)
        if txt_blob:
            for b in range(max(1, target // 1000)):
                sconn.executemany(
                    "INSERT INTO txt_vec VALUES (?,?)",
                    [(b * 1000 + k, txt_blob) for k in range(1000)],
                )
        sconn.execute("COMMIT")
        fill_ms = (time.perf_counter() - tfill) * 1000
        actual = sconn.execute("SELECT COUNT(*) FROM img_vec").fetchone()[0]

        def s_vec():
            emb = siglip.embed_texts(["a screenshot"])
            return [r[0] for r in sconn.execute("SELECT file_id, distance FROM img_vec WHERE embedding MATCH ? AND k = 20", (emb[best_text_variant][0].astype(np.float32).tobytes(),))]

        times = []
        for _ in range(args.query_runs):
            t = time.perf_counter()
            s_vec()
            times.append((time.perf_counter() - t) * 1000)
        scale_results.append(
            {
                "target": target,
                "actual_rows": actual,
                "note": "ESTIMATE — replicated rows, not real files",
                "fill_ms": round(fill_ms),
                "vec_query_p50_ms": pct(times, 50),
                "vec_query_p95_ms": pct(times, 95),
            }
        )
        print(f"  ~{actual:>6} rows: vec query p50 {scale_results[-1]['vec_query_p50_ms']}ms (fill {fill_ms:.0f}ms)")
        sconn.close()
        est.unlink()

    conn.close()

    # ---------------- report ----------------
    env = {
        "os": platform.platform(),
        "python": platform.python_version(),
        "cpu": platform.processor(),
        "cores": __import__("os").cpu_count(),
        "ram_gb": round(psutil.virtual_memory().total / 1e9, 1),
    }
    result = {
        "measured_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        "environment": env,
        "fts_tokenizer": fts_tokenizer,
        "corpus": {"images": len(images), "docs": len(docs), "cloud_placeholders_skipped": cloud_skipped + cloud_skipped_docs},
        "model_lock": lock,
        "rss_mb": {"models_loaded": round(rss_models_loaded), "after_indexing": round(rss_after_index)},
        "image_stage_ms": {
            "decode": {"p50": pct(t_decode, 50), "p95": pct(t_decode, 95)},
            "ocr": {"p50": pct(t_ocr, 50), "p95": pct(t_ocr, 95)},
            "clip_embed": {"p50": pct(t_clip, 50), "p95": pct(t_clip, 95)},
            "end_to_end": {"p50": pct(t_e2e, 50), "p95": pct(t_e2e, 95)},
        },
        "doc_stage_ms": {
            "extract": {"p50": pct(t_extract, 50), "p95": pct(t_extract, 95)},
            "chunk": {"p50": pct(t_chunk, 50), "p95": pct(t_chunk, 95)},
            "embed": {"p50": pct(t_embed, 50), "p95": pct(t_embed, 95)},
        },
        "cross_modal_sanity": sanity,
        "query_latency_real_corpus": timings,
        "query_latency_scale_estimates": scale_results,
        "n_tiles": n_tiles,
        "disclaimer": "Scale numbers are replication estimates. Real-corpus numbers are measurements.",
    }
    (RESULTS / "results.json").write_text(json.dumps(result, indent=2))

    lines = [
        "## Measured results (spike)",
        "",
        f"- Machine: {env['cpu']} · {env['cores']} cores · {env['ram_gb']} GB RAM · {env['os']}",
        f"- Corpus: {len(images)} real images, {len(docs)} real documents · FTS5 {fts_tokenizer} · measured {result['measured_at']}",
        "",
        "| Stage | p50 | p95 |",
        "|---|---|---|",
    ]
    for stage, d in [("image decode", result["image_stage_ms"]["decode"]), ("OCR", result["image_stage_ms"]["ocr"]), ("CLIP embed", result["image_stage_ms"]["clip_embed"]), ("image end-to-end", result["image_stage_ms"]["end_to_end"]), ("doc extract", result["doc_stage_ms"]["extract"]), ("doc chunk", result["doc_stage_ms"]["chunk"]), ("doc embed", result["doc_stage_ms"]["embed"])]:
        lines.append(f"| {stage} | {d['p50']} ms | {d['p95']} ms |")
    lines += ["", "| Query (real corpus, %d tiles) | p50 | p95 |" % n_tiles, "|---|---|---|"]
    for t in timings:
        lines.append(f"| {t['label']} | {t['p50_ms']} ms | {t['p95_ms']} ms |")
    lines += ["", "| Scale estimate (replicated) | vec query p50 | note |", "|---|---|---|"]
    for s in scale_results:
        lines.append(f"| ~{s['actual_rows']} rows | {s['vec_query_p50_ms']} ms | replication estimate |")
    lines += ["", "Cross-modal sanity (text-from-OCR retrieving its own image; 1 = perfect):"]
    for name, s in sanity.items():
        lines.append(f"- `{name}` text embedding: mean rank {s['mean_rank_of_correct']} of {s['n']}, top-1 rate {s['top1_rate']}")
    (RESULTS / "results.md").write_text("\n".join(lines) + "\n")
    print(f"\nwrote {RESULTS / 'results.json'} and {RESULTS / 'results.md'}")


if __name__ == "__main__":
    main()
