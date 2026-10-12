"""Replica of spike.py's image loop, per-file, first 10 files.

Same classes, same discovery order — but prints each stage's time and the
OCR text length per file, so we can see whether the spike's near-zero
timings are real or a measurement artifact.
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
siglip, minilm, ocr = SigLIP({}), MiniLM({}), OCR()
from PIL import Image  # noqa: E402

images, skipped = discover(
    {".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".tiff"}, 10, include_cloud=False
)
print(f"first 10 of discovery (cloud skipped: {skipped}):")
for i, path in enumerate(images):
    te = time.perf_counter()
    img = Image.open(path)
    img.load()
    t_dec = time.perf_counter() - te
    t1 = time.perf_counter()
    text = ocr.text(img)
    t_ocr = time.perf_counter() - t1
    t2 = time.perf_counter()
    emb = siglip.embed_images([img])
    t_clip = time.perf_counter() - t2
    print(
        f"  [{i}] {path.name[:40]:<42} {img.size} "
        f"dec {t_dec * 1000:7.1f}ms  ocr {t_ocr * 1000:8.1f}ms ({len(text):4d} chars)  "
        f"clip {t_clip * 1000:7.1f}ms  vec {emb.shape}"
    )
