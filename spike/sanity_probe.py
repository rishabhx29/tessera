"""Fast cross-modal probe: which SigLIP output pairing actually works?

The full spike's sanity check scored the pooler variant at mean-rank 17.1/40
(near-random) and crashed on a shape-mismatched second candidate. This probe
embeds 40 real images under every candidate vision embedding and every
candidate text embedding, and reports the retrieval rank for each pairing.
The winner gets baked into models.py.
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
from spike import discover  # noqa: E402

from models import MODELS_DIR, OCR, SigLIP, ensure_models  # noqa: E402

ensure_models(progress=lambda _m: None)
siglip, ocr = SigLIP({}), OCR()
from PIL import Image  # noqa: E402

images, _ = discover(
    {".png", ".jpg", ".jpeg", ".webp", ".bmp"}, 40, include_cloud=False
)
if len(images) < 8:
    raise SystemExit(f"need >= 8 images, found {len(images)}")

pil_images, texts = [], []
for p in images:
    try:
        pil_images.append(Image.open(p))
    except Exception:
        continue
print(f"probe corpus: {len(pil_images)} images")

print("running OCR on probe corpus ...")
for img in pil_images:
    texts.append(ocr.text(img)[:64])
keep = [i for i, t in enumerate(texts) if t.strip()]
pil_images = [pil_images[i] for i in keep]
texts = [texts[i] for i in keep]
print(f"{len(pil_images)} with OCR text")

# ---- vision candidates ----
np_ = np
batch = siglip._preprocess  # noqa: F841
vision_outs = siglip.vision.run(
    None, {siglip.vision.get_inputs()[0].name: np.concatenate([siglip._preprocess(im) for im in pil_images], axis=0)}
)
vision_cands = {}
two_d = [np.asarray(o, dtype=np.float32) for o in vision_outs if np.asarray(o).ndim == 2]
three_d = [np.asarray(o, dtype=np.float32) for o in vision_outs if np.asarray(o).ndim == 3]
if two_d:
    vision_cands["vision_2d"] = two_d[0]
if three_d:
    lh = three_d[0]
    vision_cands["vision_lh_mean"] = lh.mean(axis=1)
print("vision candidates:", {k: v.shape for k, v in vision_cands.items()})

# ---- text candidates ----
enc = siglip.tokenizer.encode_batch(texts)
ids = np.array([e.ids for e in enc], dtype=np.int64)
mask = np.array([e.attention_mask for e in enc], dtype=np.int64)
feed = {"input_ids": ids}
input_names = {i.name for i in siglip.text.get_inputs()}
if "attention_mask" in input_names:
    feed["attention_mask"] = mask
if "token_type_ids" in input_names:
    feed["token_type_ids"] = np.zeros_like(ids)
text_outs = siglip.text.run(None, feed)
text_cands = {}
t_two_d = [np.asarray(o, dtype=np.float32) for o in text_outs if np.asarray(o).ndim == 2]
t_three_d = [np.asarray(o, dtype=np.float32) for o in text_outs if np.asarray(o).ndim == 3]
if t_two_d:
    text_cands["text_2d"] = t_two_d[0]
if t_three_d:
    lh = t_three_d[0]
    text_cands["text_lh_mean"] = lh.mean(axis=1)
    lens = mask.sum(axis=1) - 1
    text_cands["text_lh_eos"] = lh[np.arange(len(lens)), lens]
print("text candidates:", {k: v.shape for k, v in text_cands.items()})


def norm(a):
    return a / np.linalg.norm(a, axis=-1, keepdims=True)


best = None
for vname, vemb in vision_cands.items():
    for tname, temb in text_cands.items():
        sims = norm(vemb) @ norm(temb).T
        n = sims.shape[0]
        ranks = [int((sims[:, r] > sims[r, r]).sum() + 1) for r in range(n)]
        mean_rank = float(np.mean(ranks))
        top1 = sum(1 for r in ranks if r == 1) / n
        marker = ""
        if best is None or mean_rank < best[0]:
            best = (mean_rank, vname, tname)
            marker = "  <-- best"
        print(
            f"  {vname:<14} x {tname:<12} mean rank {mean_rank:6.2f}/{n}  top1 {top1:.3f}{marker}"
        )

print(f"\nWINNER: {best[1]} x {best[2]} (mean rank {best[0]:.2f})")
