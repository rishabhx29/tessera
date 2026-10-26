#!/usr/bin/env python3
"""Build the mixed corpus for the ticket-12 milestone gate.

The corpus is **generated and copied into a temporary directory**; it is never
committed. Real files (screenshots, photographs, markdown, PDFs, DOCX) are
copied from the paths given on the command line so the gate exercises the
formats the product is for; synthetic files are added to reach the target size
and to carry known-answer markers that cannot collide with copied text.

Usage (from the repository root):

    python scripts/make-gate-corpus.py --out %TEMP%\\tessera-gate --total 5000 ^
        --text-source docs --text-source %USERPROFILE%\\.agents\\skills ^
        --pdf-source %USERPROFILE%\\Downloads ^
        --docx-source %USERPROFILE%\\Downloads ^
        --image-source %USERPROFILE%\\OneDrive\\Pictures\\Screenshots ^
        --image-source %USERPROFILE%\\Pictures\\Wallpapers

Writes `<out>/corpus` and `<out>/manifest.json`. The manifest records the counts
and the known-answer phrases, so the measuring script does not have to guess
them. Only Python's standard library plus PIL, reportlab and python-docx are
used; all three are present in the machine's Python.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import shutil
import string
import sys
import zipfile
from pathlib import Path

TEXT_EXT = {".md", ".txt"}
IMAGE_EXT = {".png", ".jpg", ".jpeg"}

PDF_MARKER = "Zephyrine codicil 7Q4X governs the lunar freight arbitration"
DOCX_MARKER = "Kestrel amendment 8M2V replaces the interim custodian"
OCR_MARKER = "NORTHWIND LEDGER RECONCILIATION 5521"


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True, help="directory to create the corpus under")
    parser.add_argument("--total", type=int, default=5000)
    parser.add_argument("--text-source", action="append", default=[])
    parser.add_argument("--pdf-source", action="append", default=[])
    parser.add_argument("--docx-source", action="append", default=[])
    parser.add_argument("--image-source", action="append", default=[])
    parser.add_argument("--seed", type=int, default=12)
    return parser.parse_args(argv)


def files_under(roots: list[str], extensions: set[str], max_bytes: int):
    """Files under `roots` with one of `extensions`, deterministically ordered."""
    found: list[Path] = []
    for root in roots:
        base = Path(root)
        if not base.exists():
            continue
        if base.is_file():
            candidates = [base]
        else:
            candidates = [p for p in base.rglob("*") if p.is_file()]
        for path in candidates:
            if path.suffix.lower() not in extensions:
                continue
            try:
                if path.stat().st_size > max_bytes:
                    continue
            except OSError:
                continue
            found.append(path)
    found.sort(key=lambda p: str(p).lower())
    return found


def spread(items: list[Path], count: int) -> list[Path]:
    """At most `count` items, evenly spaced through a sorted list."""
    if count <= 0 or not items:
        return []
    if len(items) <= count:
        return list(items)
    step = len(items) / count
    return [items[int(i * step)] for i in range(count)]


def copy_many(items: list[Path], dest: Path, prefix: str) -> int:
    dest.mkdir(parents=True, exist_ok=True)
    written = 0
    for index, source in enumerate(items):
        target = dest / f"{prefix}-{index:04d}{source.suffix.lower()}"
        try:
            shutil.copyfile(source, target)
        except OSError as error:
            # A failed copy can leave an empty or partial target. Removing it is
            # what keeps a bad source from becoming a corpus file while the
            # manifest still counts only the copies that succeeded.
            try:
                target.unlink(missing_ok=True)
            except OSError:
                pass
            print(f"skip {source}: {error}", file=sys.stderr)
        else:
            written += 1
    return written


def slug_of(path: Path) -> str:
    parts = [part for part in path.resolve().parts if part not in ("\\", "/", ":")]
    tail = parts[-2:] if len(parts) >= 2 else parts
    slug = "-".join(tail).lower()
    slug = "".join(ch if ch.isalnum() else "-" for ch in slug).strip("-")
    return (slug or "source")[:48]


def copy_real_images(roots: list[str], dest: Path, take: int) -> tuple[int, dict[str, int]]:
    """Copy real images into a folder per source, so a hit can be attributed.

    The folder name is the source directory's tail, which is what lets the
    measuring script say "the description query found the wallpaper set" without
    hashing files afterwards.
    """
    labelled: list[tuple[str, Path]] = []
    for root in roots:
        slug = slug_of(Path(root))
        for path in files_under([root], IMAGE_EXT, 8_000_000):
            labelled.append((slug, path))
    labelled.sort(key=lambda pair: (pair[0], str(pair[1]).lower()))
    written: dict[str, int] = {}
    for slug, source in spread_labelled(labelled, take):
        folder = dest / slug
        folder.mkdir(parents=True, exist_ok=True)
        target = folder / f"{slug}-{written.get(slug, 0):04d}{source.suffix.lower()}"
        try:
            shutil.copyfile(source, target)
        except OSError as error:
            try:
                target.unlink(missing_ok=True)
            except OSError:
                pass
            print(f"skip {source}: {error}", file=sys.stderr)
        else:
            written[slug] = written.get(slug, 0) + 1
    return sum(written.values()), written


def spread_labelled(items: list[tuple[str, Path]], count: int) -> list[tuple[str, Path]]:
    if count <= 0 or not items:
        return []
    if len(items) <= count:
        return list(items)
    step = len(items) / count
    return [items[int(index * step)] for index in range(count)]



def generate_text(dest: Path, count: int, rng: random.Random) -> None:
    """Long, varied text files so chunking and the text arm get real work."""
    dest.mkdir(parents=True, exist_ok=True)
    topics = [
        "procurement", "inventory", "shipping", "payroll", "facilities",
        "research", "customer support", "security review", "travel", "audit",
    ]
    for index in range(count):
        topic = topics[index % len(topics)]
        paragraphs = []
        for paragraph in range(rng.randint(3, 9)):
            sentences = []
            for _ in range(rng.randint(3, 7)):
                sentences.append(
                    f"The {topic} working group reviewed item {rng.randint(1000, 9999)} "
                    f"on day {rng.randint(1, 28)} and recorded decision "
                    f"{''.join(rng.choices(string.ascii_uppercase, k=3))}-{rng.randint(10, 99)}."
                )
            paragraphs.append(" ".join(sentences))
        (dest / f"note-{index:04d}.md").write_text(
            f"# {topic.title()} note {index}\n\n" + "\n\n".join(paragraphs) + "\n",
            encoding="utf-8",
        )


def generate_pdfs(dest: Path, count: int) -> None:
    from reportlab.lib.pagesizes import LETTER
    from reportlab.pdfgen import canvas

    dest.mkdir(parents=True, exist_ok=True)
    for index in range(count):
        path = dest / f"report-{index:04d}.pdf"
        pdf = canvas.Canvas(str(path), pagesize=LETTER)
        for page in range(3):
            pdf.setFont("Helvetica", 11)
            y = 740
            pdf.drawString(72, y, f"Quarterly operations report {index}, page {page + 1}")
            y -= 24
            for line in range(26):
                pdf.drawString(
                    72,
                    y,
                    f"Line {line:02d}: routine paragraph for report {index}, page {page + 1}.",
                )
                y -= 18
            if index == 7 and page == 2:
                pdf.setFont("Helvetica-Bold", 13)
                pdf.drawString(72, 120, PDF_MARKER + ".")
            pdf.showPage()
        pdf.save()


def generate_docx(dest: Path, count: int) -> None:
    import docx

    dest.mkdir(parents=True, exist_ok=True)
    for index in range(count):
        document = docx.Document()
        document.add_heading(f"Memorandum {index}", level=1)
        for paragraph in range(4):
            document.add_paragraph(
                f"Paragraph {paragraph} of memorandum {index}: the committee noted the "
                "position and asked for a further paper at the next meeting."
            )
        if index == 42:
            document.add_paragraph(DOCX_MARKER + ".")
        document.save(str(dest / f"memo-{index:04d}.docx"))


def generate_screenshots(dest: Path, count: int) -> None:
    """Rendered-text screenshots: real PNGs whose text the OCR arm can read."""
    from PIL import Image, ImageDraw, ImageFont

    dest.mkdir(parents=True, exist_ok=True)
    font = ImageFont.load_default(size=34)
    small = ImageFont.load_default(size=22)
    for index in range(count):
        image = Image.new("RGB", (1100, 760), (246, 247, 249))
        draw = ImageDraw.Draw(image)
        draw.rectangle([0, 0, 1100, 86], fill=(24, 36, 58))
        draw.text((32, 24), "ACME SUPPLIER PORTAL", fill=(255, 255, 255), font=font)
        draw.text((32, 130), f"Purchase order PO-{5000 + index}", fill=(20, 20, 20), font=font)
        draw.text((32, 200), f"Vendor: Northwind Trading {index:03d}", fill=(40, 40, 40), font=small)
        draw.text((32, 250), f"Amount: {1200 + index}.00 EUR", fill=(40, 40, 40), font=small)
        draw.text((32, 300), f"Invoice date: 2026-03-{(index % 28) + 1:02d}", fill=(40, 40, 40), font=small)
        draw.text((32, 350), "Status: awaiting approval", fill=(40, 40, 40), font=small)
        if index == 17:
            draw.text((32, 470), OCR_MARKER, fill=(150, 20, 20), font=font)
            draw.text((32, 540), "Reconciliation complete", fill=(40, 40, 40), font=small)
        image.save(dest / f"screenshot-{index:04d}.png")


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    rng = random.Random(args.seed)
    out = Path(args.out).resolve()
    corpus = out / "corpus"
    if corpus.exists():
        shutil.rmtree(corpus)
    corpus.mkdir(parents=True)

    total = args.total
    # A realistic personal mix: mostly documents, a large image minority.
    text_target = int(total * 0.40)
    pdf_target = int(total * 0.14)
    docx_target = int(total * 0.10)
    image_target = total - text_target - pdf_target - docx_target

    text_files = files_under(args.text_source, TEXT_EXT, 200_000)
    text_existing = copy_many(spread(text_files, text_target), corpus / "text" / "real", "text")
    generate_text(corpus / "text" / "generated", text_target - text_existing, rng)

    pdf_files = files_under(args.pdf_source, {".pdf"}, 5_000_000)
    # The PDF known answer sits in generated report 7, so every run has to
    # generate at least eight PDFs; real files may not take that room.
    pdf_reserve = 8
    pdf_existing = copy_many(
        spread(pdf_files, min(len(pdf_files), 40, max(0, pdf_target - pdf_reserve))),
        corpus / "pdf" / "real",
        "real-pdf",
    )
    generate_pdfs(corpus / "pdf" / "generated", max(pdf_target - pdf_existing, pdf_reserve))

    docx_files = files_under(args.docx_source, {".docx"}, 5_000_000)
    # The Word known answer is memo-0042.docx, so generate at least 43 memos even
    # when a small --total would ask for fewer.
    docx_existing = copy_many(
        spread(docx_files, min(len(docx_files), 20, max(0, docx_target - 43))),
        corpus / "docx" / "real",
        "real-docx",
    )
    generate_docx(corpus / "docx" / "generated", max(docx_target - docx_existing, 43))

    # At full size leave 650 images of headroom for generated screenshots. Below
    # that the subtraction goes negative, so a small run splits its image share
    # instead and still copies real images. Eighteen generated screenshots are
    # the floor that keeps screenshot-0017 (the OCR known answer) present.
    image_generated_floor = 18
    if image_target > 650:
        image_real_budget = image_target - 650
    else:
        image_real_budget = max(0, image_target - image_generated_floor)
    image_existing, image_sets = copy_real_images(
        args.image_source, corpus / "images" / "real", image_real_budget
    )
    generate_screenshots(
        corpus / "images" / "generated",
        max(image_target - image_existing, image_generated_floor),
    )

    counts = {
        "text": sum(1 for _ in (corpus / "text").rglob("*") if _.is_file()),
        "pdf": sum(1 for _ in (corpus / "pdf").rglob("*") if _.is_file()),
        "docx": sum(1 for _ in (corpus / "docx").rglob("*") if _.is_file()),
        "image": sum(1 for _ in (corpus / "images").rglob("*") if _.is_file()),
    }
    manifest = {
        "corpus": str(corpus),
        "total": sum(counts.values()),
        "counts": counts,
        "copied_real": {
            "text": text_existing,
            "pdf": pdf_existing,
            "docx": docx_existing,
            "image": image_existing,
        },
        "image_sets": image_sets,
        "known_answers": {
            "pdf_marker": PDF_MARKER,
            "pdf_file": "report-0007.pdf",
            "docx_marker": DOCX_MARKER,
            "docx_file": "memo-0042.docx",
            "ocr_marker": OCR_MARKER,
            "ocr_file": "screenshot-0017.png",
            "image_description": "a comic book superhero in a red suit",
            "image_expected_set": "pictures-wallpapers",
            "fused_query": "northwind ledger reconciliation",
            "fused_expected": "screenshot-0017.png",
        },
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(json.dumps(manifest, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
