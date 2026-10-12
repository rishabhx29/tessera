"""What are the rec model's raw output values? logits / log_softmax / probs?"""

import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).parent))
import ocr_bakeoff as ob  # noqa: E402

paths = ob.ensure_models()
v6 = ob.PaddleV6Tiny(paths)

img = Image.new("RGB", (400, 48), (255, 255, 255))
d = ImageDraw.Draw(img)
d.text((10, 12), "invoice 4471", fill=(0, 0, 0))
crop = np.asarray(img)[:, :, ::-1].copy()
pre = v6._rec_preprocess(crop)
logits = v6.rec.run(None, {v6.rec.get_inputs()[0].name: pre[None]})[0][0]

print(f"shape: {logits.shape}")
print(f"global min {logits.min():.4f}  max {logits.max():.4f}  mean {logits.mean():.4f}")

# a step that decodes to a real character
steps = logits.argmax(axis=-1)
t = next(i for i, s in enumerate(steps) if s != 0)
row = logits[t]
top = np.argsort(row)[-5:][::-1]
print(f"\nstep {t} (decodes to class {steps[t]} = {v6.alphabet[steps[t]]!r}):")
print(f"  row min {row.min():.4f} max {row.max():.4f} sum {row.sum():.4f}")
for c in top:
    print(f"  class {c:>6} ({v6.alphabet[c]!r:>8}): raw {row[c]:.6f}  exp(raw) {np.exp(row[c]):.3e}")

# is exp(row) already a probability distribution?
e = np.exp(row)
print(f"\nsum(exp(row)) = {e.sum():.6f}  (1.0 => output is already log_softmax)")
# softmax the row properly
sm = np.exp(row - row.max())
sm /= sm.sum()
print(f"proper softmax max = {sm.max():.6f}")
