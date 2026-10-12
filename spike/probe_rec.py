"""Probe: why do real-screenshot crops fail the 0.5 score gate?

Prints raw (text, score) per detected line, no gate, for three variants:
  A. as-is (tight boxes, pad=2)
  B. dilated det mask (3x3, 2 iterations) before connected components
  C. bigger crop padding (15% of box height)
"""

from __future__ import annotations

import sys
from pathlib import Path

import cv2
import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
import ocr_bakeoff as ob  # noqa: E402


def detect_variant(v6, img, dilate_iters=0):
    x, (sx, sy) = v6._det_preprocess(img)
    prob = v6.det.run(None, {v6.det.get_inputs()[0].name: x})[0][0, 0]
    mask = (prob > ob.DET_BIN_THRESHOLD).astype(np.uint8)
    if dilate_iters:
        kernel = np.ones((3, 3), np.uint8)
        mask = cv2.dilate(mask, kernel, iterations=dilate_iters)
    n, labels, stats, _ = cv2.connectedComponentsWithStats(mask, 8)
    boxes = []
    for i in range(1, n):
        x0, y0, bw, bh, area = stats[i]
        if area < 10 or bw < 3 or bh < 3:
            continue
        boxes.append(
            (
                int(x0 / sx),
                int(y0 / sy),
                int(min((x0 + bw) / sx, img.shape[1])),
                int(min((y0 + bh) / sy, img.shape[0])),
            )
        )
    return boxes


def recognize_raw(v6, img, boxes, pad_frac=0.0):
    results = []
    crops = []
    for x0, y0, x1, y1 in boxes:
        pad = int(max(2, pad_frac * (y1 - y0)))
        crop = img[max(0, y0 - pad): min(img.shape[0], y1 + pad),
                   max(0, x0 - pad): min(img.shape[1], x1 + pad)]
        pre = v6._rec_preprocess(crop)
        if pre is not None:
            crops.append((pre, (x0, y0, x1, y1)))
    if not crops:
        return results
    for pre, box in crops:
        logits = v6.rec.run(None, {v6.rec.get_inputs()[0].name: pre[None]})[0][0]
        text, score = v6._ctc(logits)
        results.append((box, text, score))
    return results


def main():
    paths = ob.ensure_models()
    v6 = ob.PaddleV6Tiny(paths)
    files = ob.find_screenshots(5)
    img = None
    for f in files:
        decoded = cv2.imread(str(f))
        if decoded is not None:
            img = decoded[:, :, ::-1]  # RGB
            name = f.name
            break
    print(f"probe image: {name}  {img.shape[1]}x{img.shape[0]}")

    variants = [
        ("A. as-is", dict(dilate_iters=0, pad_frac=0.0)),
        ("B. dilated mask", dict(dilate_iters=2, pad_frac=0.0)),
        ("C. dilated + 15% pad", dict(dilate_iters=2, pad_frac=0.15)),
    ]
    for label, kw in variants:
        boxes = detect_variant(v6, img, dilate_iters=kw["dilate_iters"])
        results = recognize_raw(v6, img, boxes, pad_frac=kw["pad_frac"])
        print(f"\n== {label}: {len(boxes)} boxes ==")
        for box, text, score in results[:10]:
            print(f"  {score:.3f}  '{text[:60]}'")
        scores = [s for _, _, s in results]
        if scores:
            print(f"  score: median {np.median(scores):.3f}  max {max(scores):.3f}  above 0.5: {sum(1 for s in scores if s >= 0.5)}/{len(scores)}")


if __name__ == "__main__":
    main()
