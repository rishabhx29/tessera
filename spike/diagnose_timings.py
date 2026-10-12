"""Diagnostic: are the spike's image-stage timings real?

Results.md claims OCR p50=2.5ms and CLIP embed p50=0.2ms, which is impossible
for ViT-B int8 CPU inference. This prints the corpus composition (file sizes,
pixel dimensions) and times the stages explicitly, per file, with repeats.
"""

from __future__ import annotations

import sys
import time
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
from spike import discover  # noqa: E402

from models import MiniLM, OCR, SigLIP, ensure_models  # noqa: E402

ensure_models(progress=lambda _m: None)
siglip, ocr, minilm = SigLIP({}), OCR(), MiniLM({})
from PIL import Image  # noqa: E402

images, _ = discover({".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".tiff"}, 120, include_cloud=False)

print("\n== corpus composition ==")
dims, sizes = [], []
for p in images:
    try:
        with Image.open(p) as im:
            dims.append(im.size)
    except Exception as e:
        dims.append(f"ERR {e}")
    sizes.append(p.stat().st_size)
    if len(dims) <= 15:
        print(f"  {p.name[:44]:<46} {sizes[-1] / 1024:8.1f} KB  {dims[-1]}")
kbs = [s / 1024 for s in sizes]
print(f"  ... total {len(images)} files, median {np.median(kbs):.1f} KB, max {max(kbs):.1f} KB")
pixels = [d[0] * d[1] for d in dims if isinstance(d, tuple)]
print(f"  dimensions: median {np.median(pixels) / 1000:.0f} kpixels, max {max(pixels) / 1000:.0f} kpixels")

print("\n== explicit stage timing on the 8 largest images ==")
real = sorted(images, key=lambda p: p.stat().st_size, reverse=True)[:8]
for p in real:
    t = time.perf_counter()
    img = Image.open(p)
    img.load()
    t_dec = (time.perf_counter() - t) * 1000
    t = time.perf_counter()
    text = ocr.text(img)
    t_ocr = (time.perf_counter() - t) * 1000
    t = time.perf_counter()
    _ = siglip.embed_images([img])
    t_clip = (time.perf_counter() - t) * 1000
    print(
        f"  {p.name[:36]:<38} {img.size}  decode {t_dec:6.1f}ms  OCR {t_ocr:7.1f}ms ({len(text):4d} chars)  CLIP {t_clip:7.1f}ms"
    )

print("\n== text embed timing (x5) ==")
for q in ["the invoice from the airline", "a screenshot", "meeting notes q3 roadmap planning"]:
    ts = []
    for _ in range(5):
        t = time.perf_counter()
        siglip.embed_texts([q])
        ts.append((time.perf_counter() - t) * 1000)
    print(f"  siglip text  '{q[:32]}': p50 {np.median(ts):6.1f}ms")
for q in ["the invoice from the airline", "a document about budgets"]:
    ts = []
    for _ in range(5):
        t = time.perf_counter()
        minilm.embed_texts([q])
        ts.append((time.perf_counter() - t) * 1000)
    print(f"  minilm text  '{q[:32]}': p50 {np.median(ts):6.1f}ms")
