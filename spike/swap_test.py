"""Model-swap test — the ADR-0004 remedy path, measured.

Spike verdict was FAIL: RapidOCR PP-OCRv5 at 2.5-8 s/image and SigLIP-base at
~200 ms/image blow the <1500 ms save->searchable target. Two swaps to test:

  1. OCR: RapidOCR (PP-OCRv5, ONNX) -> Windows.Media.Ocr (WinRT).
  2. Image arm: SigLIP-base int8 -> MobileCLIP2-S0 int8
     (felixhrdyn/mobileclip2-s0-onnx, vision+text+tokenizer, MIT).

For each: latency on the same 10 real files the replica timed, plus the
synthetic retrieval sanity test that decided the SigLIP extraction (if the
swap degrades retrieval to random, it is not a swap, it is a regression).
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
from spike import discover  # noqa: E402

ROOT = Path(__file__).parent
MODELS = ROOT / "models" / "mobileclip2"
MODELS.mkdir(parents=True, exist_ok=True)
LOCK = ROOT / "model-lock.json"

URLS = {
    "vision": "https://huggingface.co/felixhrdyn/mobileclip2-s0-onnx/resolve/main/vision_model_quantized.onnx",
    "text": "https://huggingface.co/felixhrdyn/mobileclip2-s0-onnx/resolve/main/text_model_quantized.onnx",
    "tokenizer": "https://huggingface.co/felixhrdyn/mobileclip2-s0-onnx/resolve/main/tokenizer.json",
}


def sha256_file(p: Path) -> str:
    import hashlib

    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def ensure_models():
    import os

    lock = json.loads(LOCK.read_text()) if LOCK.exists() else {}
    for name, url in URLS.items():
        dest = MODELS / Path(url).name
        key = f"mobileclip2_{name}"
        if not dest.exists():
            print(f"  downloading {key} ...")
            urllib.request.urlretrieve(url, dest)
        got = sha256_file(dest)
        lock[key] = {"sha256": got, "url": url, "bytes": dest.stat().st_size}
        print(f"  {key}: {got[:16]}... ({dest.stat().st_size / 1e6:.1f} MB)")
    LOCK.write_text(json.dumps(lock, indent=2) + "\n")


# ---------------------------------------------------------------- Windows OCR
async def win_ocr_setup():
    from winsdk.windows.globalization import ApplicationLanguages, Language
    from winsdk.windows.media.ocr import OcrEngine

    candidates = ["en-US", "en"]
    try:
        candidates += list(ApplicationLanguages.languages)
    except Exception:
        pass
    tried = set()
    for tag in candidates:
        if tag in tried:
            continue
        tried.add(tag)
        try:
            engine = OcrEngine.try_create_from_language(Language(tag))
        except Exception:
            continue
        if engine is not None:
            print(f"  WinOCR engine ready for language: {tag}")
            return engine
    return None


async def win_ocr_file(path: Path, engine) -> str:
    from winsdk.windows.graphics.imaging import BitmapDecoder
    from winsdk.windows.storage import StorageFile

    file = await StorageFile.get_file_from_path_async(str(path))
    stream = await file.open_read_async()
    decoder = await BitmapDecoder.create_async(stream)
    bitmap = await decoder.get_software_bitmap_async()
    result = await engine.recognize_async(bitmap)
    return result.text


async def run_win_ocr(files):
    engine = await win_ocr_setup()
    if engine is None:
        print("  WinOCR: no recognizer language available — cannot test")
        return None, None
    times, lens = [], []
    for p in files:
        t = time.perf_counter()
        text = await win_ocr_file(p, engine)
        times.append((time.perf_counter() - t) * 1000)
        lens.append(len(text))
    return times, lens


# ------------------------------------------------------------- MobileCLIP2-S0
class MobileCLIP2:
    def __init__(self):
        import onnxruntime as ort
        from tokenizers import Tokenizer

        so = ort.SessionOptions()
        so.intra_op_num_threads = 8
        self.vision = ort.InferenceSession(str(MODELS / "vision_model_quantized.onnx"), so, providers=["CPUExecutionProvider"])
        self.text = ort.InferenceSession(str(MODELS / "text_model_quantized.onnx"), so, providers=["CPUExecutionProvider"])
        self.tokenizer = Tokenizer.from_file(str(MODELS / "tokenizer.json"))
        self.tokenizer.enable_padding(length=64, pad_id=0, pad_token="<|endoftext|>")
        self.tokenizer.enable_truncation(64)

    def _preprocess(self, img, size=256):
        import numpy as np

        from PIL import Image as PILImage

        mean = np.array([0.48145466, 0.4578275, 0.40821073], dtype=np.float32) * 255
        std = np.array([0.26862954, 0.26130258, 0.27577711], dtype=np.float32) * 255
        im = img.convert("RGB").resize((size, size), PILImage.Resampling.BICUBIC)
        arr = np.asarray(im, dtype=np.float32).transpose(2, 0, 1)
        arr = (arr - mean[:, None, None]) / std[:, None, None]
        return arr[None, ...]

    def embed_images(self, imgs, size=256):
        np_ = np
        batch = np_.concatenate([self._preprocess(im, size) for im in imgs], axis=0)
        outs = self.vision.run(None, {self.vision.get_inputs()[0].name: batch})
        for o in outs:
            a = np_.asarray(o, dtype=np.float32)
            if a.ndim == 2:
                return a / np_.linalg.norm(a, axis=-1, keepdims=True)
        a = np_.asarray(outs[0], dtype=np.float32)
        return (a.mean(axis=1)) / np_.linalg.norm(a.mean(axis=1), axis=-1, keepdims=True)

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
        raise RuntimeError(f"no 2-D text output; shapes: {[np_.asarray(o).shape for o in outs]}")


def main():
    from PIL import Image, ImageDraw

    from models import ensure_models as ensure_siglip  # not used here, keeps imports honest  # noqa: F401

    print("== downloading MobileCLIP2-S0 quantized ==")
    ensure_models()

    images, _ = discover({".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".tiff"}, 10, include_cloud=False)
    print(f"== Windows.Media.Ocr on {len(images)} real files ==")
    times, lens = asyncio.run(run_win_ocr(images))
    if times:
        print(f"  p50 {np.median(times):.1f} ms  p95 {np.percentile(times, 95):.1f} ms  mean {np.mean(times):.1f} ms")
        print(f"  text lengths: {lens}")

    print("== MobileCLIP2-S0 on the same 10 files ==")
    clip = MobileCLIP2()
    pil = []
    for p in images:
        try:
            with Image.open(p) as im:
                im.load()
                pil.append(im.copy())
        except Exception:
            continue
    ts = []
    for im in pil:
        t = time.perf_counter()
        clip.embed_images([im])
        ts.append((time.perf_counter() - t) * 1000)
    print(f"  vision embed p50 {np.median(ts):.1f} ms  p95 {np.percentile(ts, 95):.1f} ms")

    ts = []
    for q in ["the invoice from the airline", "a screenshot", "meeting notes q3 roadmap planning"]:
        for _ in range(3):
            t = time.perf_counter()
            clip.embed_texts([q])
            ts.append((time.perf_counter() - t) * 1000)
    print(f"  text embed p50 {np.median(ts):.1f} ms")

    print("== retrieval sanity: MobileCLIP2-S0 synthetic (rendered text) ==")
    phrases = [
        "boarding pass seat 14A to lisbon",
        "wifi password hunter2 network",
        "error 0x8007045d windows update",
        "blood test results hemoglobin 13.1",
        "letter from the bank about overdraft",
        "recipe for banana bread ingredients",
        "ticket concert friday october 2026",
        "library card number 998877",
        "prescription amoxicillin 500mg",
        "delivery note signature required",
        "tax return assessment year 2024",
        "lease agreement apartment 12B",
        "insurance policy renewal premium",
        "meeting agenda roadmap planning",
        "shipping label tracking 1Z999",
        "school report card mathematics A",
        "hostel booking two beds ensuite",
        "phone screen cracked repair quote",
        "parking fine appeal form reference",
        "subscription cancel by december",
    ]
    random.seed(7)
    imgs = []
    for ph in phrases:
        im = Image.new("RGB", (640, 400), tuple(random.randint(20, 60) for _ in range(3)))
        dr = ImageDraw.Draw(im)
        dr.rectangle([80, 60, 560, 340], fill=(210, 210, 210))
        dr.text((120, 160), ph, fill=(10, 10, 10))
        imgs.append(im)
    for size in (256, 224):
        try:
            img_embs = clip.embed_images(imgs, size=size)
        except Exception as e:
            print(f"  size {size}: image embed failed: {e}")
            continue
        t_embs = clip.embed_texts(phrases)
        sims = img_embs @ t_embs.T
        n = sims.shape[0]
        ranks = [int((sims[:, r] > sims[r, r]).sum() + 1) for r in range(n)]
        top1 = sum(1 for r in ranks if r == 1) / n
        print(f"  size {size}: mean rank {np.mean(ranks):.2f}/{n}  top1 {top1:.3f}")

    print("\n== projected save->searchable with the swap ==")
    print(f"  decode (median of real files) + WinOCR p50 {np.median(times) if times else '?':.1f} ms + MobileCLIP p50 {np.median(ts) if ts else '?':.1f} ms")


if __name__ == "__main__":
    main()
