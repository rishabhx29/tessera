"""Debug the minimal PP-OCRv6 pipeline on ONE screenshot: which stage is broken?

Checks, in order:
  1. det output: shape, value range (probabilities vs logits?), mask coverage
  2. components found: count and sizes
  3. rec on a synthetic clean text crop: does CTC decode anything?
  4. alphabet vs model output classes: mapping sanity
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
from ocr_bakeoff import DET_BIN_THRESHOLD, DET_MAX_SIDE, PaddleV6Tiny, find_screenshots, ensure_models  # noqa: E402


def main():
    import cv2
    from PIL import Image, ImageDraw

    paths = ensure_models()
    v6 = PaddleV6Tiny(paths)

    print("== alphabet / classes ==")
    print(f"  alphabet entries: {len(v6.alphabet)}")
    print(f"  first 10: {v6.alphabet[:10]}")
    print(f"  last 5: {v6.alphabet[-5:]}")
    print(f"  rec input height: {v6.rec_h}")

    # ---- synthetic text crop through rec ----
    print("\n== rec on synthetic text crop ==")
    img = Image.new("RGB", (400, 48), (255, 255, 255))
    d = ImageDraw.Draw(img)
    d.text((10, 12), "invoice 4471", fill=(0, 0, 0))
    crop = np.asarray(img)[:, :, ::-1].copy()  # RGB -> BGR to match pipeline
    pre = v6._rec_preprocess(crop)
    logits = v6.rec.run(None, {v6.rec.get_inputs()[0].name: pre[None]})[0][0]
    print(f"  logits shape: {logits.shape}  (steps, classes)")
    text, score = v6._ctc(logits)
    print(f"  decoded: '{text}' score {score:.3f}")
    steps = logits.argmax(axis=-1)
    print(f"  argmax per step: {steps[:40]}")

    # ---- real screenshot through det ----
    print("\n== det on a real screenshot ==")
    files = find_screenshots(5)
    for f in files[:2]:
        img = cv2.imread(str(f))
        if img is None:
            continue
        x, (sx, sy) = v6._det_preprocess(img)
        out = v6.det.run(None, {v6.det.get_inputs()[0].name: x})
        prob = out[0][0, 0] if out[0].ndim == 4 else out[0][0]
        print(f"  {f.name[:40]}: det out shape {out[0].shape}, range [{out[0].min():.3f}, {out[0].max():.3f}], mean {out[0].mean():.4f}")
        for thr in (0.3, 0.5, 0.7):
            mask = (prob > thr).astype(np.uint8)
            n = int(mask.sum())
            print(f"    threshold {thr}: {100 * n / mask.size:.2f}% of pixels above")
        boxes = v6.detect(img)
        print(f"    boxes found (thr {DET_BIN_THRESHOLD}): {len(boxes)}")
        for b in boxes[:8]:
            print(f"      {b}  (w={b[2]-b[0]}, h={b[3]-b[1]})")
        lines = v6.recognize(img, boxes)
        print(f"    lines kept: {len(lines)} -> {[t for t, _ in lines][:5]}")
        break


if __name__ == "__main__":
    main()
