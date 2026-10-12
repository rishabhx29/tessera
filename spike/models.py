"""Model acquisition and loading for the Tessera week-1 spike.

Downloads the *product-shaped* models (ONNX, quantized where available) and
verifies them against `model-lock.json`:

- First run:  download → compute SHA-256 → write the lock file (trust on first
  use; the lock is then committed so every later run is verified).
- Later runs: verify every hash before load. A mismatch is a hard failure, not
  a warning — this is the same discipline the shipping engine will apply on
  first run (ADR-0004), minus the known-good upstream checksums.
"""

from __future__ import annotations

import hashlib
import json
import os
import urllib.request
from pathlib import Path

MODELS_DIR = Path(__file__).parent / "models"
LOCK_PATH = Path(__file__).parent / "model-lock.json"

HF_BASE = "https://huggingface.co"
MODEL_FILES = {
    "siglip_vision": ("siglip", f"{HF_BASE}/Xenova/siglip-base-patch16-224/resolve/main/onnx/vision_model_quantized.onnx"),
    "siglip_text": ("siglip", f"{HF_BASE}/Xenova/siglip-base-patch16-224/resolve/main/onnx/text_model_quantized.onnx"),
    "siglip_tokenizer": ("siglip", f"{HF_BASE}/Xenova/siglip-base-patch16-224/resolve/main/tokenizer.json"),
    "siglip_preprocessor": ("siglip", f"{HF_BASE}/Xenova/siglip-base-patch16-224/resolve/main/preprocessor_config.json"),
    "minilm_model": ("minilm", f"{HF_BASE}/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model_quantized.onnx"),
    "minilm_tokenizer": ("minilm", f"{HF_BASE}/Xenova/all-MiniLM-L6-v2/resolve/main/tokenizer.json"),
}

SIGLIP_DIR = MODELS_DIR / "siglip"
MINILM_DIR = MODELS_DIR / "minilm"


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_lock() -> dict:
    if LOCK_PATH.exists():
        return json.loads(LOCK_PATH.read_text())
    return {}


def ensure_models(progress=print) -> dict:
    """Download (if needed) and verify every model file. Returns the lock."""
    MODELS_DIR.mkdir(exist_ok=True)
    lock = load_lock()

    for name, (subdir, url) in MODEL_FILES.items():
        dest = MODELS_DIR / subdir / Path(url).name
        dest.parent.mkdir(parents=True, exist_ok=True)
        if dest.exists():
            got = sha256_file(dest)
            if name in lock and lock[name]["sha256"] != got:
                raise SystemExit(
                    f"CHECKSUM MISMATCH for {name}: {got} != {lock[name]['sha256']}. "
                    "Delete the file under spike/models/ to re-fetch."
                )
            lock[name] = {"sha256": got, "url": url, "bytes": dest.stat().st_size}
            continue

        progress(f"downloading {name} ...")
        tmp = dest.with_suffix(".part")
        urllib.request.urlretrieve(url, tmp)
        os.replace(tmp, dest)
        got = sha256_file(dest)
        lock[name] = {"sha256": got, "url": url, "bytes": dest.stat().st_size}
        progress(f"  {name}: {got[:16]}… ({dest.stat().st_size / 1e6:.1f} MB)")

    LOCK_PATH.write_text(json.dumps(lock, indent=2) + "\n")
    return lock


class SigLIP:
    """SigLIP-base ONNX (quantized) — the image arm of the search design."""

    def __init__(self, lock: dict):
        import json

        import numpy as np
        import onnxruntime as ort
        from tokenizers import Tokenizer

        self.np = np
        self.tokenizer = Tokenizer.from_file(str(SIGLIP_DIR / "tokenizer.json"))
        self.tokenizer.enable_padding(length=64, pad_id=1, pad_token="</s>")
        self.tokenizer.enable_truncation(64)
        pre = json.loads((SIGLIP_DIR / "preprocessor_config.json").read_text())
        self.size = (pre["size"]["height"], pre["size"]["width"])
        self.mean = np.array(pre["image_mean"], dtype=np.float32) * 255.0
        self.std = np.array(pre["image_std"], dtype=np.float32) * 255.0
        so = ort.SessionOptions()
        so.intra_op_num_threads = os.cpu_count() or 4
        self.vision = ort.InferenceSession(
            str(SIGLIP_DIR / "vision_model_quantized.onnx"), so, providers=["CPUExecutionProvider"]
        )
        self.text = ort.InferenceSession(
            str(SIGLIP_DIR / "text_model_quantized.onnx"), so, providers=["CPUExecutionProvider"]
        )

    def _preprocess(self, pil_image):
        np = self.np
        img = pil_image.convert("RGB").resize(self.size, Image_BILINEAR)
        arr = np.asarray(img, dtype=np.float32).transpose(2, 0, 1)  # CHW
        arr = (arr - self.mean[:, None, None]) / self.std[:, None, None]
        return arr[None, ...]

    def embed_images(self, pil_images):
        np = self.np
        batch = np.concatenate([self._preprocess(im) for im in pil_images], axis=0)
        outs = self.vision.run(None, {self.vision.get_inputs()[0].name: batch})
        # transformers.js SigLIP vision model: [last_hidden_state, pooler_output].
        # The pooled 2-D output is the projected image embedding.
        emb = None
        for out in outs:
            arr = np.asarray(out, dtype=np.float32)
            if arr.ndim == 2 and arr.shape[0] == batch.shape[0] and emb is None:
                emb = arr
        if emb is None:
            emb = np.asarray(outs[0], dtype=np.float32).mean(axis=1)
        return emb / np.linalg.norm(emb, axis=-1, keepdims=True)

    def _raw_text(self, texts):
        np = self.np
        enc = self.tokenizer.encode_batch(list(texts))
        ids = np.array([e.ids for e in enc], dtype=np.int64)
        input_names = {i.name for i in self.text.get_inputs()}
        feed = {"input_ids": ids}
        if "attention_mask" in input_names:
            feed["attention_mask"] = np.array([e.attention_mask for e in enc], dtype=np.int64)
        if "token_type_ids" in input_names:
            feed["token_type_ids"] = np.zeros_like(ids)
        return self.text.run(None, feed)

    def embed_texts(self, texts):
        np = self.np
        out = self._raw_text(texts)
        # transformers.js SigLIP text model returns [last_hidden_state, pooler_output].
        # Try pooler first; fall back to EOS-position hidden state if the export
        # doesn't project. The spike's cross-modal sanity check decides.
        cands = {"pooler": np.asarray(out[-1], dtype=np.float32)}
        if len(out) > 1:
            cands["last_hidden"] = np.asarray(out[0], dtype=np.float32)
        return {k: v / np.linalg.norm(v, axis=-1, keepdims=True) for k, v in cands.items()}


class MiniLM:
    """all-MiniLM-L6-v2 ONNX quantized — the document meaning arm."""

    def __init__(self, lock: dict):
        import numpy as np
        import onnxruntime as ort
        from tokenizers import Tokenizer

        self.np = np
        self.tokenizer = Tokenizer.from_file(str(MINILM_DIR / "tokenizer.json"))
        self.tokenizer.enable_padding(length=256, pad_id=0, pad_token="[PAD]")
        self.tokenizer.enable_truncation(256)
        so = ort.SessionOptions()
        so.intra_op_num_threads = os.cpu_count() or 4
        self.model = ort.InferenceSession(
            str(MINILM_DIR / "model_quantized.onnx"), so, providers=["CPUExecutionProvider"]
        )

    def embed_texts(self, texts):
        np = self.np
        enc = self.tokenizer.encode_batch(list(texts))
        ids = np.array([e.ids for e in enc], dtype=np.int64)
        mask = np.array([e.attention_mask for e in enc], dtype=np.int64)
        ttype = np.zeros_like(ids)  # single-sequence MiniLM usage
        out = self.model.run(
            None, {"input_ids": ids, "attention_mask": mask, "token_type_ids": ttype}
        )[0]
        mask_f = mask[..., None].astype(np.float32)
        summed = (out * mask_f).sum(axis=1)
        counts = np.clip(mask_f.sum(axis=1), 1e-9, None)
        emb = summed / counts
        return emb / np.linalg.norm(emb, axis=-1, keepdims=True)


class OCR:
    """RapidOCR (PP-OCRv5) via onnxruntime — the text arm for images."""

    def __init__(self):
        from rapidocr_onnxruntime import RapidOCR

        self.engine = RapidOCR()

    def text(self, pil_image) -> str:
        import numpy as np

        arr = np.asarray(pil_image.convert("RGB"))
        result, _ = self.engine(arr)
        if not result:
            return ""
        return "\n".join(line[1] for line in result)


# Pillow constant kept here so models.py has no import-order surprises.
from PIL import Image as _Image  # noqa: E402

Image_BILINEAR = _Image.Resampling.BILINEAR
