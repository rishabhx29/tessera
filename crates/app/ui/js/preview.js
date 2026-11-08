// The right sheet: the matched file and the evidence that put it here.
//
// The sheet is not modal. Focus stays where it was, the wall behind keeps its
// layout, and Escape closes. For a picture the sheet opens the page or tile the
// result named, draws the amber line marks on the picture, and lists the OCR
// lines underneath.

import { call } from "./ipc.js";
import {
  appendSegments,
  dimensions,
  el,
  kindGlyph,
  markLayer,
  segmentLines,
  thumbUrl,
} from "./dom.js";

let ctx = null;
let nodes = null;
let seq = 0;
let objectUrl = null;
let hideTimer = null;

export function initPreview(root, context) {
  ctx = context;
  nodes = {
    sheet: root,
    name: root.querySelector("#preview-name"),
    path: root.querySelector("#preview-path"),
    body: root.querySelector("#preview-body"),
    meta: root.querySelector("#preview-meta"),
    reveal: root.querySelector("#preview-reveal"),
    close: root.querySelector("#preview-close"),
  };
  nodes.close.addEventListener("click", close);
  nodes.reveal.addEventListener("click", () => revealCurrent());
}

export function isOpen() {
  return Boolean(nodes) && !nodes.sheet.hidden;
}

export function close() {
  if (!isOpen()) return;
  seq++;
  releaseImage();
  nodes.sheet.classList.remove("open");
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => {
    nodes.sheet.hidden = true;
    nodes.body.replaceChildren();
  }, 180);
}

function releaseImage() {
  if (objectUrl) {
    URL.revokeObjectURL(objectUrl);
    objectUrl = null;
  }
}

function revealCurrent() {
  const path = nodes.sheet.dataset.path;
  if (!path) return;
  call("reveal", { path }).catch((error) =>
    ctx.toast(`Could not open the folder: ${error}`, "error"),
  );
}

export async function open(row, { reveal = false } = {}) {
  if (!row || !nodes) return;
  const run = ++seq;
  releaseImage();
  clearTimeout(hideTimer);
  nodes.sheet.hidden = false;
  // Commit the from-state before adding the class, so the sheet has two states
  // to move between.
  void nodes.sheet.offsetWidth;
  nodes.sheet.classList.add("open");
  nodes.sheet.dataset.path = row.path;

  nodes.name.textContent = row.name;
  nodes.path.textContent = row.path;
  nodes.meta.textContent = metaText(row, null);
  nodes.body.replaceChildren(el("p", "preview-wait", "opening the file…"));

  if (reveal) {
    call("reveal", { path: row.path }).catch((error) =>
      ctx.toast(`Could not open the folder: ${error}`, "error"),
    );
  }

  let preview;
  try {
    preview = await call("preview", {
      path: row.path,
      query: ctx.state.query,
      page: row.page ?? null,
    });
  } catch (error) {
    if (run !== seq) return;
    nodes.body.replaceChildren(el("p", "preview-error", String(error)));
    return;
  }
  if (run !== seq) return;

  nodes.sheet.dataset.path = preview.path;
  nodes.name.textContent = preview.name;
  nodes.path.textContent = preview.path;
  nodes.body.replaceChildren();

  if (preview.image_path) {
    renderPicture(preview, row, run);
  } else {
    renderText(preview, { dropName: false });
  }

  nodes.meta.textContent = metaText(row, preview);
}

function metaText(row, preview) {
  const parts = [];
  if (preview?.kind ?? row.kind) parts.push(preview?.kind ?? row.kind);
  if (preview?.page ?? row.page) parts.push(`page ${preview?.page ?? row.page}`);
  const size = dimensions(row);
  if (size && (!preview || preview.image_path)) parts.push(size);
  return parts.join(" · ");
}

function renderPicture(preview, row, run) {
  const media = el("div", "preview-media");
  const picture = el("div", "preview-picture");
  const img = document.createElement("img");
  img.alt = preview.name;
  img.decoding = "async";
  img.draggable = false;

  const known = thumbUrl(preview.path);
  if (known) {
    img.src = known;
    img.dataset.quality = "thumbnail";
  }

  img.addEventListener("load", () => {
    if (run !== seq) return;
    updatePictureMeta(img, row, preview);
  });
  img.addEventListener("error", () => {
    if (run !== seq) return;
    if (known && img.dataset.quality === "full") {
      img.dataset.quality = "thumbnail";
      img.src = known;
      return;
    }
    img.remove();
    picture.append(el("p", "preview-error", "the picture could not be read."));
  });

  picture.append(img, markLayer(preview.marks));
  media.append(picture);
  nodes.body.append(media);
  const wait = el("p", "preview-wait", "opening the picture…");
  if (!known) nodes.body.append(wait);

  call("preview_image", { path: preview.path })
    .then((bytes) => {
      if (run !== seq) return;
      const url = URL.createObjectURL(new Blob([bytes]));
      releaseImage();
      objectUrl = url;
      img.dataset.quality = "full";
      img.src = url;
      wait.remove();
    })
    .catch(() => {
      if (run !== seq) return;
      wait.remove();
      // No thumbnail and no full image: there is nothing to show, so say so
      // in the place the picture would be.
      if (known) return;
      img.remove();
      const missing = el("div", "preview-missing");
      missing.append(
        kindGlyph(preview.kind),
        el("span", "", "this picture could not be read."),
      );
      picture.append(missing);
    });

  renderOcr(preview, row);
}

function updatePictureMeta(img, row, preview) {
  const size =
    img.naturalWidth && img.naturalHeight
      ? `${img.naturalWidth} × ${img.naturalHeight}`
      : dimensions(row);
  const parts = [preview.kind];
  if (size) parts.push(size);
  nodes.meta.textContent = parts.join(" · ");
}

function renderOcr(preview, row) {
  const lines = displayLines(preview.text, preview.name);
  if (!lines.length) {
    const size = dimensions(row);
    nodes.body.append(
      el(
        "p",
        "ocr-empty",
        size
          ? `no text in this picture. ${size} pixels.`
          : "no text in this picture.",
      ),
    );
    return;
  }
  if (hasHit(lines)) nodes.body.append(matchedLabel());
  const box = el("div", "preview-text");
  for (const line of lines) {
    const node = el("p", "ocr-line");
    appendSegments(node, line);
    box.append(node);
  }
  nodes.body.append(box);
}

function renderText(preview, { dropName }) {
  const lines = displayLines(preview.text, dropName ? preview.name : "");
  if (!lines.length) {
    nodes.body.append(
      el(
        "p",
        "ocr-empty",
        preview.image_path ? "no text in this picture." : "no text on this page.",
      ),
    );
    return;
  }
  if (hasHit(lines)) nodes.body.append(matchedLabel());
  const box = el("div", "preview-text");
  for (const line of lines) {
    const node = el("p", "ocr-line");
    appendSegments(node, line);
    box.append(node);
  }
  nodes.body.append(box);
}

function hasHit(lines) {
  return lines.some((line) => line.some((part) => part.hit));
}

function matchedLabel() {
  return el("p", "preview-label mono", "matched line");
}

// The engine's image text starts with the file's own name. That line is never
// marked and is not part of what the picture says, so the sheet drops it.
function displayLines(segments, nameToDrop) {
  let lines = segmentLines(segments);
  if (nameToDrop) {
    const first = (lines[0] ?? []).map((part) => part.text).join("").trim();
    if (first && first.toLowerCase() === String(nameToDrop).toLowerCase()) {
      lines = lines.slice(1);
    }
  }
  return lines.filter((line) => line.some((part) => part.text.trim()));
}
