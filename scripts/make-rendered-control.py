"""Render the image arm's synthetic control set (ticket 07).

The control asks the one question the real-screenshot numbers cannot: does the
image arm match a *description* to an image whose content is unambiguous? Each
image is a phrase rendered in large type on a plain background, and the query is
that phrase. This is the methodology the week-1 spike used when it measured
top-1 0.90, reproduced here so the number is now derivable from the repository.

The output is committed, because it is a fixture rather than a corpus:
    crates/core/tests/fixtures/rendered/<NN>-<slug>.png
    crates/core/tests/fixtures/rendered/control.json

Run:
    python scripts/make-rendered-control.py

The phrases are deliberately unlike each other in wording, colour and layout, so
a miss means the arm could not read the picture rather than that two images were
interchangeable. They are not a sample of anything and the file says so: this is
a control, not a benchmark corpus.
"""

import json
import pathlib

from PIL import Image, ImageDraw, ImageFont

SIDE = 448
OUT = pathlib.Path(__file__).resolve().parents[1] / "crates" / "core" / "tests" / "fixtures" / "rendered"

# (phrase, background, ink, font file, layout)
# Fonts are the ones Windows 11 always has; the fallback keeps the script
# runnable on a machine without them rather than failing with a missing file.
CASES = [
    ("a red bicycle leaning on a brick wall", "#f4efe6", "#8f2f1d", "georgia.ttf", "center"),
    ("a blue teapot on a wooden table", "#17324d", "#e8f1f8", "segoeuib.ttf", "center"),
    ("the yellow umbrella by the door", "#20201c", "#f0d24a", "trebucbd.ttf", "left"),
    ("three green apples in a paper bag", "#f7f3d9", "#3f7a34", "verdana.ttf", "center"),
    ("a silver train crossing a stone bridge", "#dfe7ec", "#324a5c", "georgia.ttf", "left"),
    ("a purple flower in a glass jar", "#fbf6ff", "#6b2f8a", "segoeuib.ttf", "center"),
    ("an orange cat asleep on a radiator", "#efe6d8", "#b45a17", "trebucbd.ttf", "center"),
    ("a black camera on a white shelf", "#fafafa", "#1d1d1d", "consola.ttf", "left"),
    ("two wooden chairs under a window", "#e9dfc9", "#5b4326", "verdana.ttf", "center"),
    ("a small boat on a calm lake", "#d7ecf7", "#1f5d7a", "georgia.ttf", "center"),
    ("a copper kettle beside a chipped mug", "#f3eee7", "#8a5a2b", "segoeuib.ttf", "left"),
    ("the brown dog waiting at the gate", "#e8efe1", "#4a3728", "trebucbd.ttf", "center"),
    ("a green cactus in a red pot", "#fdf3e7", "#2f6b3a", "verdana.ttf", "center"),
    ("an old map pinned to a wall", "#e7dcc3", "#4b3a24", "georgia.ttf", "left"),
    ("a white van parked outside a bakery", "#eef1f4", "#232323", "consola.ttf", "center"),
    ("a striped scarf hanging on a hook", "#f6e9ee", "#8f2f5a", "trebucbd.ttf", "center"),
    ("a metal key next to a folded letter", "#f5f1e6", "#3d3d3d", "segoeuib.ttf", "left"),
    ("a bowl of lemons on a tiled counter", "#fbf7e3", "#9a7b1f", "verdana.ttf", "center"),
    ("a paper lantern above a dark street", "#1b1b24", "#f2b64c", "georgia.ttf", "center"),
    ("a bluebird perched on a wire fence", "#e8f2f8", "#2a5d8a", "trebucbd.ttf", "center"),
]

FALLBACK_FONTS = ["arial.ttf", "segoeui.ttf", "calibri.ttf"]


def load_font(name, size):
    candidates = [name, *FALLBACK_FONTS]
    for candidate in candidates:
        for root in (pathlib.Path("C:/Windows/Fonts"), pathlib.Path("/usr/share/fonts/truetype/dejavu")):
            path = root / candidate
            if path.exists():
                return ImageFont.truetype(str(path), size)
    return ImageFont.load_default()


def wrap(draw, text, font, width):
    words = text.split()
    lines, current = [], ""
    for word in words:
        candidate = f"{current} {word}".strip()
        if draw.textlength(candidate, font=font) <= width or not current:
            current = candidate
        else:
            lines.append(current)
            current = word
    if current:
        lines.append(current)
    return lines


def slug(text):
    return "-".join("".join(c if c.isalnum() else " " for c in text).split())[:48]


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    manifest = []
    for index, (phrase, background, ink, font_name, layout) in enumerate(CASES, start=1):
        image = Image.new("RGB", (SIDE, SIDE), background)
        draw = ImageDraw.Draw(image)
        font = load_font(font_name, 44)
        lines = wrap(draw, phrase, font, SIDE - 56)
        line_height = 56
        total = line_height * len(lines)
        y = (SIDE - total) // 2
        for line in lines:
            width = draw.textlength(line, font=font)
            x = 28 if layout == "left" else (SIDE - width) / 2
            draw.text((x, y), line, font=font, fill=ink)
            y += line_height
        name = f"{index:02d}-{slug(phrase)}.png"
        image.save(OUT / name, optimize=True)
        manifest.append({"file": name, "phrase": phrase})

    (OUT / "control.json").write_text(
        json.dumps(
            {
                "note": "Synthetic rendered-text control for the image arm. Query each image with its phrase.",
                "images": manifest,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"wrote {len(manifest)} images and control.json to {OUT}")


if __name__ == "__main__":
    main()
