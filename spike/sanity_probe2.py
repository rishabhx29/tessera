"""Sanity probe v2 — corpus-controlled.

v1 said near-random, but on a phone *photo* corpus where OCR text is noise.
Two tests here:

A. SYNTHETIC: render N images each containing one distinctive phrase.Pairing
   image_emb vs text_emb(phrase) must retrieve correctly if the shared space
   works at all. Corpus-independent, decisive.
B. REAL SCREENSHOTS: same test on files that are actually screenshots
   (matched by name/folder), if this machine has >= 20 of them.
C. SELF-CHECKS: same image twice -> cosine ~1; duplicated image with small
   perturbation -> nearest neighbour of itself (image-image side works?).
"""

from __future__ import annotations

import random
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
from spike import discover  # noqa: E402

from models import MODELS_DIR, OCR, SigLIP, ensure_models  # noqa: E402

ensure_models(progress=lambda _m: None)
siglip = SigLIP({})


def norm(a):
    return a / np.linalg.norm(a, axis=-1, keepdims=True)


def retrieval_stats(img_embs, text_embs):
    sims = norm(img_embs) @ norm(text_embs).T
    n = sims.shape[0]
    ranks = [int((sims[:, r] > sims[r, r]).sum() + 1) for r in range(n)]
    return float(np.mean(ranks)), sum(1 for r in ranks if r == 1) / n, n


print("== C. self-checks ==")
from PIL import Image, ImageDraw  # noqa: E402

img = Image.new("RGB", (640, 400), (30, 30, 30))
d = ImageDraw.Draw(img)
d.rectangle([100, 80, 540, 320], fill=(200, 200, 200))
d.text((150, 160), "invoice payment confirmation 4471", fill=(0, 0, 0))
e1 = norm(siglip.embed_images([img]))
e2 = norm(siglip.embed_images([img.rotate(1)]))  # tiny perturbation
print(f"  same image, rotated 1deg: cosine = {float(e1 @ e2.T):.4f} (expect ~0.99)")
noise = Image.effect_noise((640, 400), 40).convert("RGB")
e3 = norm(siglip.embed_images([noise]))
print(f"  vs pure noise image: cosine = {float(e1 @ e3.T):.4f} (expect clearly lower)")

print("== A. synthetic rendered-text images ==")
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
img_embs = np.vstack(siglip.embed_images(imgs))
variants = siglip.embed_texts(phrases)
for name, t_embs in variants.items():
    mr, top1, n = retrieval_stats(img_embs, t_embs)
    print(f"  synthetic: {name:<12} mean rank {mr:6.2f}/{n}  top1 {top1:.3f}")
img_img = norm(img_embs) @ norm(img_embs).T
ranks = [int((img_img[:, r] > img_img[r, r]).sum() + 1) for r in range(len(phrases))]
print(f"  synthetic: image-image (control) mean rank {np.mean(ranks):.2f}/{len(phrases)}")

print("== B. real screenshots on this machine ==")
roots = [Path.home()]
cands = []
for root in roots:
    for base in ("Pictures", "OneDrive", "Desktop", "Downloads", "Documents", "AppData/Local/Temp"):
        p = root / base
        if not p.exists():
            continue
        try:
            for f in p.rglob("*"):
                name = f.name.lower()
                if f.is_file() and f.suffix.lower() in {".png", ".jpg", ".jpeg"} and (
                    "screenshot" in name or "screen shot" in name or "snip" in name or "screenshot" in str(f.parent).lower()
                ):
                    cands.append(f)
        except OSError:
            continue
        if len(cands) > 200:
            break
cands = cands[:40]
print(f"  found {len(cands)} screenshot-like files")
if len(cands) >= 10:
    from models import MODELS_DIR as _M  # noqa: F401

    ocr = OCR()
    pil, texts = [], []
    for f in cands:
        try:
            im = Image.open(f)
            im.load()
        except Exception:
            continue
        t = ocr.text(im)[:64]
        if not t.strip():
            continue
        pil.append(im)
        texts.append(t)
    print(f"  {len(pil)} with OCR text")
    if len(pil) >= 10:
        img_embs = np.vstack(siglip.embed_images(pil))
        for name, t_embs in siglip.embed_texts(texts).items():
            mr, top1, n = retrieval_stats(img_embs, t_embs)
            print(f"  screenshots: {name:<12} mean rank {mr:6.2f}/{n}  top1 {top1:.3f}")
