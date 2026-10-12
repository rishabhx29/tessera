"""OCR bake-off: the three candidate engines on the same 40 real screenshots.

  1. Windows.Media.Ocr (native fast path)      — measured before: ~50 ms p50
  2. RapidOCR PP-OCRv5, package defaults        — measured before: ~2.6-3.3 s p50
  3. PP-OCRv6 det-tiny + rec-small ONNX, with gyotaku's tuning:
     downscale-only to ~1 megapixel, axis-aligned boxes, batched recognition.
     Models and SHA-256 checksums pinned from gyotaku's models.rs (which took
     them from RapidOCR's ModelScope exports) — the exact artifacts the
     shipping engine would use.

This script re-measures all three on the same corpus in one session, plus a
det/rec time split for #3. Quality proxy: characters extracted per file.
"""

from __future__ import annotations

import asyncio
import json
import sys
import time
import urllib.request
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
ROOT = Path(__file__).parent
MODELS = ROOT / "models" / "ppocrv6"
MODELS.mkdir(parents=True, exist_ok=True)
LOCK = ROOT / "model-lock.json"

# Exact URLs and checksums pinned from gyotaku crates/ocr/src/models.rs.
MODELS_TO_FETCH = {
    "det": (
        "PP-OCRv6_det_tiny.onnx",
        [
            "https://github.com/xevrion/gyotaku/releases/download/models-v1/PP-OCRv6_det_tiny.onnx",
            "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv6/det/PP-OCRv6_det_tiny.onnx",
        ],
        "f42c0fbd294d95eac1a550e131b277dac97462c8025fa4b6c3cec1b7894bd3d5",
    ),
    "rec": (
        "PP-OCRv6_rec_small.onnx",
        [
            "https://github.com/xevrion/gyotaku/releases/download/models-v1/PP-OCRv6_rec_small.onnx",
            "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv6/rec/PP-OCRv6_rec_small.onnx",
        ],
        "6f327246b50388f3c176ae304bd95767ea6dc0c9ae92153ef8cbe210b3c14884",
    ),
}

DET_MAX_SIDE = 1280  # ~1 megapixel for 16:9; downscale only
DET_BIN_THRESHOLD = 0.3
DET_MEAN = np.array([0.485, 0.456, 0.406], dtype=np.float32)
DET_STD = np.array([0.229, 0.224, 0.225], dtype=np.float32)


def sha256_file(p: Path) -> str:
    import hashlib

    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def ensure_models():
    lock = json.loads(LOCK.read_text()) if LOCK.exists() else {}
    paths = {}
    for key, (fname, urls, sha) in MODELS_TO_FETCH.items():
        dest = MODELS / fname
        if not dest.exists():
            for url in urls:
                try:
                    print(f"  downloading {fname} from {url.split('/')[2]} ...")
                    urllib.request.urlretrieve(url, dest)
                    break
                except Exception as e:
                    print(f"    failed: {e}")
            if not dest.exists():
                raise SystemExit(f"could not download {fname}")
        got = sha256_file(dest)
        if got != sha:
            raise SystemExit(f"CHECKSUM MISMATCH for {fname}: {got} != {sha}")
        lock[f"ppocrv6_{key}"] = {"sha256": got, "bytes": dest.stat().st_size, "urls": urls}
        print(f"  {fname}: verified ({dest.stat().st_size / 1e6:.1f} MB)")
        paths[key] = dest
    LOCK.write_text(json.dumps(lock, indent=2) + "\n")
    return paths


class PaddleV6Tiny:
    """Minimal det+rec pipeline with gyotaku's tuning knobs. This is a
    measurement harness; the Rust engine implements this properly."""

    def __init__(self, paths):
        import onnxruntime as ort

        so = ort.SessionOptions()
        so.intra_op_num_threads = 8
        so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
        self.det = ort.InferenceSession(str(paths["det"]), so, providers=["CPUExecutionProvider"])
        self.rec = ort.InferenceSession(str(paths["rec"]), so, providers=["CPUExecutionProvider"])
        meta = self.rec.get_modelmeta().custom_metadata_map
        if "character" not in meta:
            raise SystemExit(f"rec model has no character metadata; keys: {list(meta)}")
        # Paddle convention: class 0 is CTC blank, class s maps to alphabet[s].
        # The final class is the space character Paddle appends after the
        # charset, which splitlines() drops from the metadata string.
        self.alphabet = ["blank"] + meta["character"].splitlines() + [" "]
        self.blank = 0
        rec_in = self.rec.get_inputs()[0].shape  # e.g. ['x', 3, 48, 'W']
        self.rec_h = int(rec_in[2])
        self.t_det = self.t_rec = 0.0

    def _det_preprocess(self, img):
        h, w = img.shape[:2]
        scale = min(1.0, DET_MAX_SIDE / max(h, w))
        nw, nh = max(32, int(round(w * scale / 32)) * 32), max(32, int(round(h * scale / 32)) * 32)
        import cv2

        resized = cv2.resize(img, (nw, nh), interpolation=cv2.INTER_AREA if scale < 1 else cv2.INTER_LINEAR)
        x = resized.astype(np.float32) / 255.0
        x = (x - DET_MEAN) / DET_STD
        return x.transpose(2, 0, 1)[None], (nw / w, nh / h)

    def detect(self, img):
        import cv2

        x, (sx, sy) = self._det_preprocess(img)
        t = time.perf_counter()
        prob = self.det.run(None, {self.det.get_inputs()[0].name: x})[0][0, 0]
        self.t_det += time.perf_counter() - t
        mask = (prob > DET_BIN_THRESHOLD).astype(np.uint8)
        n, labels, stats, _ = cv2.connectedComponentsWithStats(mask, 8)
        boxes = []
        for i in range(1, n):
            x0, y0, bw, bh, area = stats[i]
            if area < 10 or bw < 3 or bh < 3:
                continue
            # scale back to original coordinates (axis-aligned, gyotaku-style)
            boxes.append(
                (
                    int(x0 / sx),
                    int(y0 / sy),
                    int(min((x0 + bw) / sx, img.shape[1])),
                    int(min((y0 + bh) / sy, img.shape[0])),
                )
            )
        return boxes

    def _rec_preprocess(self, crop):
        import cv2

        h, w = crop.shape[:2]
        if h < 2 or w < 2:
            return None
        nh = self.rec_h
        nw = max(16, int(round(w * nh / h)))
        nw = min(nw, 1920)
        line = cv2.resize(crop, (nw, nh), interpolation=cv2.INTER_AREA if nw < w else cv2.INTER_LINEAR)
        x = line.astype(np.float32) / 255.0
        x = (x - 0.5) / 0.5
        return x.transpose(2, 0, 1)

    def recognize(self, img, boxes):
        """Batch lines by similar width, CTC decode."""
        t = time.perf_counter()
        crops = []
        for x0, y0, x1, y1 in boxes:
            pad = 2
            crop = img[
                max(0, y0 - pad) : min(img.shape[0], y1 + pad),
                max(0, x0 - pad) : min(img.shape[1], x1 + pad),
            ]
            pre = self._rec_preprocess(crop)
            if pre is not None:
                crops.append(pre)
        texts = []
        if crops:
            order = sorted(range(len(crops)), key=lambda i: crops[i].shape[2])
            batch, batches = [], []
            total = 0
            for i in order:
                wpx = crops[i].shape[2]
                if batch and (total + wpx > 9600 or len(batch) >= 24):
                    batches.append(batch)
                    batch, total = [], 0
                batch.append(i)
                total += wpx
            if batch:
                batches.append(batch)
            out = [None] * len(crops)
            for b in batches:
                maxw = max(crops[i].shape[2] for i in b)
                buf = np.zeros((len(b), 3, self.rec_h, maxw), dtype=np.float32)
                for j, i in enumerate(b):
                    buf[j, :, :, : crops[i].shape[2]] = crops[i]
                logits = self.rec.run(None, {self.rec.get_inputs()[0].name: buf})[0]
                for j, i in enumerate(b):
                    out[i] = self._ctc(logits[j])
            texts = out
        self.t_rec += time.perf_counter() - t
        result = []
        for (x0, y0, x1, y1), read in zip(boxes, texts):
            if read is None:
                continue
            text, score = read
            if score >= 0.5 and len(text.strip()) > 1:
                result.append((text.strip(), score))
        return result

    def _ctc(self, logits):
        """Greedy CTC: argmax per step, collapse repeats, drop blank.

        This export emits per-step probabilities (rows sum to 1), not raw
        logits; softmaxing again crushes 0.94 into 0.0001. Detect and adapt."""
        steps = logits.argmax(axis=-1)
        if np.allclose(logits.sum(axis=-1), 1.0, atol=1e-3):
            probs = logits
        else:
            probs = self._softmax(logits)
        chars, scores, prev = [], [], self.blank
        for t, s in enumerate(steps):
            if s != self.blank and s != prev:
                if s < len(self.alphabet):
                    chars.append(self.alphabet[s])
                    scores.append(probs[t, s])
            prev = s
        text = "".join(chars)
        score = float(np.mean(scores)) if scores else 0.0
        return text, score

    @staticmethod
    def _softmax(x, axis=-1):
        e = np.exp(x - x.max(axis=axis, keepdims=True))
        return e / e.sum(axis=axis, keepdims=True)

    def read(self, img):
        # Paddle models expect RGB; cv2 decodes BGR. Convert once, here.
        img = img[:, :, ::-1]
        boxes = self.detect(img)
        lines = self.recognize(img, boxes)
        return "\n".join(t for t, _ in lines)


def find_screenshots(limit=40):
    out = []
    for base in ("Pictures", "OneDrive", "Desktop", "Downloads", "Documents"):
        p = Path.home() / base
        if not p.exists():
            continue
        try:
            for f in p.rglob("*"):
                name = f.name.lower()
                if (
                    f.is_file()
                    and f.suffix.lower() in {".png", ".jpg", ".jpeg"}
                    and ("screenshot" in name or "screen shot" in name or "snip" in name or "screenshot" in str(f.parent).lower())
                ):
                    out.append(f)
        except OSError:
            continue
        if len(out) > limit * 3:
            break
    return out[:limit]


async def win_ocr_times(files):
    from winsdk.windows.globalization import Language
    from winsdk.windows.graphics.imaging import BitmapDecoder
    from winsdk.windows.media.ocr import OcrEngine
    from winsdk.windows.storage import StorageFile

    engine = OcrEngine.try_create_from_language(Language("en-US"))
    if engine is None:
        return None, None
    times, chars = [], []
    for p in files:
        t = time.perf_counter()
        file = await StorageFile.get_file_from_path_async(str(p))
        stream = await file.open_read_async()
        decoder = await BitmapDecoder.create_async(stream)
        bitmap = await decoder.get_software_bitmap_async()
        result = await engine.recognize_async(bitmap)
        times.append((time.perf_counter() - t) * 1000)
        chars.append(len(result.text))
    return times, chars


def main():
    import cv2
    from rapidocr_onnxruntime import RapidOCR

    print("== models ==")
    paths = ensure_models()

    files = find_screenshots()
    print(f"== corpus: {len(files)} real screenshots ==")
    images = []
    for f in files:
        img = cv2.imread(str(f))
        if img is not None:
            images.append((f, img))
    print(f"  {len(images)} decoded")

    print("== 1. Windows.Media.Ocr ==")
    win_times, win_chars = asyncio.run(win_ocr_times(files))
    if win_times:
        print(f"  p50 {np.median(win_times):.1f} ms  p95 {np.percentile(win_times, 95):.1f} ms  mean chars {np.mean(win_chars):.0f}")

    print("== 2. RapidOCR PP-OCRv5 (package defaults) ==")
    rapid = RapidOCR()
    r_times, r_chars = [], []
    for f, img in images:
        t = time.perf_counter()
        res, _ = rapid(img)
        r_times.append((time.perf_counter() - t) * 1000)
        r_chars.append(sum(len(line[1]) for line in res) if res else 0)
    print(f"  p50 {np.median(r_times):.1f} ms  p95 {np.percentile(r_times, 95):.1f} ms  mean chars {np.mean(r_chars):.0f}")

    print("== 3. PP-OCRv6 tiny det + small rec, gyotaku tuning ==")
    v6 = PaddleV6Tiny(paths)
    v_times, v_chars, v_lines = [], [], []
    v6.t_det = v6.t_rec = 0.0
    for f, img in images:
        t = time.perf_counter()
        text = v6.read(img)
        v_times.append((time.perf_counter() - t) * 1000)
        v_chars.append(len(text))
        v_lines.append(len(text.splitlines()))
    print(
        f"  p50 {np.median(v_times):.1f} ms  p95 {np.percentile(v_times, 95):.1f} ms  "
        f"mean chars {np.mean(v_chars):.0f}  mean lines {np.mean(v_lines):.1f}"
    )
    print(f"  det time total {v6.t_det * 1000:.0f} ms, rec time total {v6.t_rec * 1000:.0f} ms over {len(images)} files")

    print("\n== summary (same machine, same %d real screenshots) ==" % len(images))
    rows = [
        ("Windows.Media.Ocr (native)", win_times, win_chars),
        ("RapidOCR PP-OCRv5 defaults", r_times, r_chars),
        ("PP-OCRv6 tiny+small, tuned", v_times, v_chars),
    ]
    print(f"  {'engine':<32} {'p50 ms':>9} {'p95 ms':>9} {'mean chars':>11}")
    for name, ts, chs in rows:
        if ts:
            print(f"  {name:<32} {np.median(ts):>9.1f} {np.percentile(ts, 95):>9.1f} {np.mean(chs):>11.0f}")


if __name__ == "__main__":
    main()
