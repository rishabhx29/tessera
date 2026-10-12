"""Swap test 2 — the decisive A/B on real screenshots.

Part 1: Windows.Media.Ocr vs RapidOCR on this machine's 40 real screenshots:
        latency for both, plus how much text each extracts (quality proxy).
Part 2: image arm candidates for the <1500 ms save->searchable target:
        - CLIP ViT-B/32 int8 (Xenova mirror, onnxruntime-compatible)
        Measured on the same 40 screenshots, plus:
        - synthetic retrieval sanity (20 rendered-text images)
        - real-screenshot sanity (OCR text -> its own image)

Also projects the end-to-end: decode + OCR + embed.
"""

from __future__ import annotations

import asyncio
import json
import random
import sys
import time
import urllib.request
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
ROOT = Path(__file__).parent
MODELS = ROOT / "models" / "clip32"
MODELS.mkdir(parents=True, exist_ok=True)
LOCK = ROOT / "model-lock.json"

URLS = {
    "vision": "https://huggingface.co/Xenova/clip-vit-base-patch32/resolve/main/onnx/vision_model_quantized.onnx",
    "text": "https://huggingface.co/Xenova/clip-vit-base-patch32/resolve/main/onnx/text_model_quantized.onnx",
    "tokenizer": "https://huggingface.co/Xenova/clip-vit-base-patch32/resolve/main/tokenizer.json",
    "preprocessor": "https://huggingface.co/Xenova/clip-vit-base-patch32/resolve/main/preprocessor_config.json",
}


def sha256_file(p: Path) -> str:
    import hashlib

    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def ensure_models():
    lock = json.loads(LOCK.read_text()) if LOCK.exists() else {}
    for name, url in URLS.items():
        dest = MODELS / Path(url).name
        key = f"clip32_{name}"
        if not dest.exists():
            print(f"  downloading {key} ...")
            urllib.request.urlretrieve(url, dest)
        got = sha256_file(dest)
        lock[key] = {"sha256": got, "url": url, "bytes": dest.stat().st_size}
        print(f"  {key}: {got[:16]}... ({dest.stat().st_size / 1e6:.1f} MB)")
    LOCK.write_text(json.dumps(lock, indent=2) + "\n")


class CLIP32:
    def __init__(self):
        import onnxruntime as ort
        from tokenizers import Tokenizer

        so = ort.SessionOptions()
        so.intra_op_num_threads = 8
        self.vision = ort.InferenceSession(str(MODELS / "vision_model_quantized.onnx"), so, providers=["CPUExecutionProvider"])
        self.text = ort.InferenceSession(str(MODELS / "text_model_quantized.onnx"), so, providers=["CPUExecutionProvider"])
        self.tokenizer = Tokenizer.from_file(str(MODELS / "tokenizer.json"))
        self.tokenizer.enable_padding(length=77, pad_id=49407, pad_token="<|endoftext|>")
        self.tokenizer.enable_truncation(77)

    def embed_images(self, imgs):
        np_ = np
        import json as _json

        from PIL import Image as PILImage

        pre = _json.loads((MODELS / "preprocessor_config.json").read_text())
        size = (pre["size"], pre["size"])
        mean = np_.array(pre["image_mean"], dtype=np.float32) * 255
        std = np_.array(pre["image_std"], dtype=np.float32) * 255
        batch = []
        for im in imgs:
            arr = np_.asarray(im.convert("RGB").resize(size, PILImage.Resampling.BICUBIC), dtype=np.float32).transpose(2, 0, 1)
            batch.append((arr - mean[:, None, None]) / std[:, None, None])
        x = np_.concatenate(batch, axis=0)
        outs = self.vision.run(None, {self.vision.get_inputs()[0].name: x})
        for o in outs:
            a = np_.asarray(o, dtype=np.float32)
            if a.ndim == 2:
                return a / np_.linalg.norm(a, axis=-1, keepdims=True)
        a = np_.asarray(outs[0], dtype=np.float32)
        m = a.mean(axis=1)
        return m / np_.linalg.norm(m, axis=-1, keepdims=True)

    def embed_texts(self, texts):
        np_ = np
        enc = self.tokenizer.encode_batch(list(texts))
        ids = np_.array([e.ids for e in enc], dtype=np.int64)
        feed = {"input_ids": ids}
        names = {i.name for i in self.text.get_inputs()}
        if "attention_mask" in names:
            feed["attention_mask"] = np_.array([e.attention_mask for e in enc], dtype=np.int64)
        outs = self.text.run(None, feed)
        for o in outs:
            a = np_.asarray(o, dtype=np.float32)
            if a.ndim == 2:
                return a / np_.linalg.norm(a, axis=-1, keepdims=True)
        raise RuntimeError("no 2-D text output")


def find_screenshots(limit=40):
    roots = [Path.home()]
    out = []
    for root in roots:
        for base in ("Pictures", "OneDrive", "Desktop", "Downloads", "Documents"):
            p = root / base
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


def main():
    from PIL import Image, ImageDraw

    from models import OCR

    ensure_models()
    files = find_screenshots()
    print(f"== corpus: {len(files)} real screenshots ==")

    pil = []
    for f in files:
        try:
            with Image.open(f) as im:
                im.load()
                pil.append((f, im.copy()))
        except Exception:
            continue
    print(f"  {len(pil)} decoded")

    # ---------------- Part 1: OCR A/B ----------------
    from winsdk.windows.globalization import Language
    from winsdk.windows.media.ocr import OcrEngine

    engine = OcrEngine.try_create_from_language(Language("en-US"))
    rapid = OCR().engine

    async def win_one(img):
        import io

        from winsdk.windows.graphics.imaging import BitmapDecoder
        from winsdk.windows.storage import StorageFile

        path = img[0]
        file = await StorageFile.get_file_from_path_async(str(path))
        stream = await file.open_read_async()
        decoder = await BitmapDecoder.create_async(stream)
        bitmap = await decoder.get_software_bitmap_async()
        result = await engine.recognize_async(bitmap)
        return result.text

    print("== Part 1: OCR A/B (per file) ==")
    win_times, rapid_times, win_chars, rapid_chars = [], [], [], []
    win_only_missed = 0
    for i, (path, img) in enumerate(pil):
        t = time.perf_counter()
        wtext = asyncio.run(win_one((path, img)))
        win_times.append((time.perf_counter() - t) * 1000)
        win_chars.append(len(wtext))
        t = time.perf_counter()
        rresult, _ = rapid(np.asarray(img.convert("RGB")))
        rapid_times.append((time.perf_counter() - t) * 1000)
        rtext = "\n".join(line[1] for line in rresult) if rresult else ""
        rapid_chars.append(len(rtext))
        if win_chars[-1] < 20 <= rapid_chars[-1]:
            win_only_missed += 1
        if i < 10:
            print(
                f"  {path.name[:36]:<38} win {win_times[-1]:6.1f}ms ({win_chars[-1]:4d} ch)  "
                f"rapid {rapid_times[-1]:7.1f}ms ({rapid_chars[-1]:4d} ch)"
            )
    print(f"  TOTAL ({len(pil)} files):")
    print(f"    Windows.Media.Ocr: p50 {np.median(win_times):.1f} ms  p95 {np.percentile(win_times, 95):.1f} ms  mean chars {np.mean(win_chars):.0f}")
    print(f"    RapidOCR:          p50 {np.median(rapid_times):.1f} ms  p95 {np.percentile(rapid_times, 95):.1f} ms  mean chars {np.mean(rapid_chars):.0f}")
    print(f"    files where RapidOCR found >=20 chars and WinOCR found <20: {win_only_missed}/{len(pil)}")

    # ---------------- Part 2: image arm ----------------
    print("== Part 2: image arm candidates on the same screenshots ==")
    clip = CLIP32()
    imgs_only = [im for _, im in pil]
    ts = []
    for im in imgs_only[:40]:
        t = time.perf_counter()
        clip.embed_images([im])
        ts.append((time.perf_counter() - t) * 1000)
    print(f"  CLIP ViT-B/32 vision embed: p50 {np.median(ts):.1f} ms  p95 {np.percentile(ts, 95):.1f} ms")
    ts = []
    for q in ["the invoice from the airline", "a screenshot", "meeting notes q3 roadmap planning"]:
        for _ in range(3):
            t = time.perf_counter()
            clip.embed_texts([q])
            ts.append((time.perf_counter() - t) * 1000)
    print(f"  CLIP ViT-B/32 text embed:   p50 {np.median(ts):.1f} ms")

    phrases = [
        "boarding pass seat 14A to lisbon", "wifi password hunter2 network",
        "error 0x8007045d windows update", "blood test results hemoglobin 13.1",
        "letter from the bank about overdraft", "recipe for banana bread ingredients",
        "ticket concert friday october 2026", "library card number 998877",
        "prescription amoxicillin 500mg", "delivery note signature required",
        "tax return assessment year 2024", "lease agreement apartment 12B",
        "insurance policy renewal premium", "meeting agenda roadmap planning",
        "shipping label tracking 1Z999", "school report card mathematics A",
        "hostel booking two beds ensuite", "phone screen cracked repair quote",
        "parking fine appeal form reference", "subscription cancel by december",
    ]
    random.seed(7)
    syn = []
    for ph in phrases:
        im = Image.new("RGB", (640, 400), tuple(random.randint(20, 60) for _ in range(3)))
        dr = ImageDraw.Draw(im)
        dr.rectangle([80, 60, 560, 340], fill=(210, 210, 210))
        dr.text((120, 160), ph, fill=(10, 10, 10))
        syn.append(im)
    img_embs = clip.embed_images(syn)
    t_embs = clip.embed_texts(phrases)
    sims = img_embs @ t_embs.T
    n = sims.shape[0]
    ranks = [int((sims[:, r] > sims[r, r]).sum() + 1) for r in range(n)]
    print(f"  synthetic retrieval: mean rank {np.mean(ranks):.2f}/{n}  top1 {sum(1 for r in ranks if r == 1) / n:.3f}")

    # real screenshots with OCR text (WinOCR's text) as queries
    pairs = [(im, wt) for (im, wt) in zip(imgs_only, [asyncio.run(win_one(x)) for x in pil]) if wt.strip()]
    if len(pairs) >= 10:
        img_embs = clip.embed_images([p[0] for p in pairs])
        t_embs = clip.embed_texts([p[1][:64] for p in pairs])
        sims = img_embs @ t_embs.T
        n = sims.shape[0]
        ranks = [int((sims[:, r] > sims[r, r]).sum() + 1) for r in range(n)]
        print(f"  real-screenshot retrieval: mean rank {np.mean(ranks):.2f}/{n}  top1 {sum(1 for r in ranks if r == 1) / n:.3f}  (n={len(pairs)})")

    print("\n== projected save->searchable (per file) ==")
    print(f"  decode ~10-100ms + WinOCR p50 {np.median(win_times):.0f} ms + CLIP32 p50 {np.median(ts):.0f} ms"
          f"  =>  ~{10 + np.median(win_times) + np.median(ts) if False else 10 + np.median(win_times):.0f}+{np.median(ts):.0f} ms")


if __name__ == "__main__":
    main()
