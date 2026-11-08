// Small DOM and formatting helpers shared by the views.
//
// Nothing here knows about search, lenses or the engine. The reason wording and
// the kind glyphs live here because both the wall and the result rows show them
// and neither view may import the other.

import { call } from "./ipc.js";
import { layoutRows } from "../lib/justified.js";

export function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

const numberFormat = new Intl.NumberFormat();

export function count(value) {
  return numberFormat.format(Number(value) || 0);
}

export function plural(value, one, many) {
  return Number(value) === 1 ? one : many;
}

export function formatMillis(ms) {
  if (!Number.isFinite(ms)) return "";
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${Math.round(ms)} ms`;
}

export function relativeTime(mtimeNs) {
  const then = Number(mtimeNs) / 1e6;
  if (!Number.isFinite(then) || then <= 0) return "";
  const seconds = Math.max(0, (Date.now() - then) / 1000);
  if (seconds < 60) return "now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
  if (seconds < 604800) return `${Math.floor(seconds / 86400)}d`;
  return new Date(then).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function basename(path) {
  const parts = String(path ?? "").split(/[\\/]/);
  return parts[parts.length - 1] || String(path ?? "");
}

export function folderName(path) {
  const parts = String(path ?? "").replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || String(path ?? "");
}

export function dimensions(row) {
  return row?.width && row?.height ? `${row.width} × ${row.height}` : "";
}

export function appendSegments(target, segments) {
  for (const segment of segments ?? []) {
    const text = String(segment?.text ?? "");
    if (!text) continue;
    if (segment.hit) {
      const mark = el("mark");
      mark.textContent = text;
      target.append(mark);
    } else {
      target.append(document.createTextNode(text));
    }
  }
}

// Split highlighted segments into display lines. The engine's tile text uses
// newlines; the preview wants one line per OCR band, with the same highlights.
export function segmentLines(segments) {
  const lines = [[]];
  for (const segment of segments ?? []) {
    const parts = String(segment?.text ?? "").split("\n");
    parts.forEach((part, index) => {
      if (index > 0) lines.push([]);
      if (part) lines[lines.length - 1].push({ text: part, hit: Boolean(segment.hit) });
    });
  }
  return lines;
}

// The amber evidence boxes, in the same normalized coordinates for the wall
// and the preview. Decorative by contract: the reason text carries the meaning.
export function markLayer(marks) {
  const layer = el("span", "marks");
  layer.setAttribute("aria-hidden", "true");
  for (const mark of marks ?? []) {
    const left = Math.max(0, Math.min(1, Number(mark.x0)));
    const top = Math.max(0, Math.min(1, Number(mark.y0)));
    const right = Math.max(left, Math.min(1, Number(mark.x1)));
    const bottom = Math.max(top, Math.min(1, Number(mark.y1)));
    if (![left, top, right, bottom].every(Number.isFinite)) continue;
    const box = el("span", "mark-box");
    box.style.left = `${left * 100}%`;
    box.style.top = `${top * 100}%`;
    box.style.width = `${Math.max(0.4, (right - left) * 100)}%`;
    box.style.height = `${Math.max(0.4, (bottom - top) * 100)}%`;
    layer.append(box);
  }
  return layer;
}

export const KIND_ICONS = {
  image:
    '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2.75" y="3.75" width="14.5" height="12.5" rx="2"></rect><circle cx="7.2" cy="8" r="1.4"></circle><path d="M3.5 14.5 8 10l3 2.75 3-3.25 2.5 2.5"></path></svg>',
  pdf:
    '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 2.75h6.5l3.5 3.5v11H5z"></path><path d="M11.5 2.75v3.5h3.5"></path><path d="M7.5 11.25h5M7.5 13.75h5"></path></svg>',
  doc: '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 2.75h6.5l3.5 3.5v11H5z"></path><path d="M11.5 2.75v3.5h3.5"></path></svg>',
  text: '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 5.5h12M4 10h12M4 14.5h8"></path></svg>',
};

export function kindGroup(kind) {
  const name = String(kind || "").toLowerCase();
  if (name === "image" || isImageExtension(name)) return "image";
  if (name === "pdf") return "pdf";
  if (name === "docx" || name === "doc") return "doc";
  return "text";
}

const IMAGE_EXTENSIONS = new Set([
  "png",
  "jpg",
  "jpeg",
  "webp",
  "bmp",
  "gif",
  "tif",
  "tiff",
]);

function isImageExtension(name) {
  return IMAGE_EXTENSIONS.has(name);
}

export function isImageKind(kind) {
  return kindGroup(kind) === "image";
}

export function kindGlyph(kind) {
  const icon = el("span", "kind-glyph");
  icon.innerHTML = KIND_ICONS[kindGroup(kind)];
  icon.setAttribute("aria-hidden", "true");
  return icon;
}

// Why a hit surfaced, in the order the arms matter to a picture search. Only
// the `all` lens shows this: in a single-question lens the lens is the reason.
export function reasonFor(row, query) {
  const arms = row.arms ?? [];
  const parts = [];
  if (arms.includes("image")) parts.push("looks like this");
  if (arms.includes("keyword")) {
    parts.push(row.name_only ? "name matches" : `says “${String(query ?? "").trim()}”`);
  }
  if (arms.includes("meaning")) parts.push("mentions this");
  return parts.join(" · ");
}

let toastNode = null;
let toastTimer = null;

export function initToast(node) {
  toastNode = node;
}

export function toast(message, kind = "info") {
  if (!toastNode) {
    console.info(message);
    return;
  }
  toastNode.textContent = message;
  toastNode.dataset.kind = kind;
  toastNode.hidden = false;
  void toastNode.offsetWidth;
  toastNode.classList.add("shown");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toastNode.classList.remove("shown");
    toastTimer = setTimeout(() => {
      toastNode.hidden = true;
    }, 200);
  }, 4200);
}

export function renderNotes(container, notes) {
  container.replaceChildren();
  for (const note of notes ?? []) {
    const line = el("p", note?.error ? "note error" : "note");
    line.textContent = String(note?.text ?? note ?? "");
    container.append(line);
  }
}

// -- pictures ---------------------------------------------------------------
//
// A tile is the picture, not a card: the frame fills the button, the caption
// only appears over the picture on hover or keyboard focus. The fallback stays
// under the image, so a slow or broken thumbnail is a neutral named frame
// rather than a hole in the wall.

const THUMB_CACHE_LIMIT = 420;
const thumbCache = new Map(); // path -> { url } | { failed: true } | { promise }
let thumbObserver = null;

function rememberThumb(path, entry) {
  thumbCache.delete(path);
  thumbCache.set(path, entry);
  while (thumbCache.size > THUMB_CACHE_LIMIT) {
    const [oldest, value] = thumbCache.entries().next().value;
    if (value?.url) URL.revokeObjectURL(value.url);
    thumbCache.delete(oldest);
  }
}

export function thumbUrl(path) {
  return thumbCache.get(path)?.url ?? null;
}

export function clearThumbnailCache() {
  for (const value of thumbCache.values()) {
    if (value?.url) URL.revokeObjectURL(value.url);
  }
  thumbCache.clear();
}

export function loadThumb(path) {
  const known = thumbCache.get(path);
  if (known?.url || known?.failed) return Promise.resolve(known.url ?? null);
  if (known?.promise) return known.promise;
  const promise = call("thumbnail", { path })
    .then((bytes) => {
      const url = URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" }));
      rememberThumb(path, { url });
      return url;
    })
    .catch((error) => {
      // A picture that cannot be decoded keeps its name and stays openable;
      // the payload, not the thumbnail, is the answer.
      console.debug("thumbnail unavailable", path, error);
      rememberThumb(path, { failed: true });
      return null;
    });
  rememberThumb(path, { promise });
  return promise;
}

function paintFrame(frame, url) {
  if (!url) {
    frame.dataset.state = "failed";
    return;
  }
  let img = frame.querySelector("img");
  if (!img) {
    img = document.createElement("img");
    img.alt = "";
    img.decoding = "async";
    img.draggable = false;
    img.addEventListener("load", () => {
      if (frame.isConnected) frame.dataset.state = "ready";
    });
    img.addEventListener("error", () => {
      if (frame.isConnected) frame.dataset.state = "failed";
      img.remove();
    });
    frame.prepend(img);
  }
  if (img.dataset.url === url) {
    if (img.complete && img.naturalWidth > 0) frame.dataset.state = "ready";
    return;
  }
  img.dataset.url = url;
  img.src = url;
}

function paintPath(path, url) {
  for (const frame of document.querySelectorAll(".tile-frame")) {
    if (frame.dataset.path === path) paintFrame(frame, url);
  }
}

function thumbObserverFor() {
  if (thumbObserver) return thumbObserver;
  thumbObserver = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        const frame = entry.target;
        thumbObserver.unobserve(frame);
        const path = frame.dataset.path;
        if (!path) continue;
        const known = thumbCache.get(path);
        if (known?.url) {
          paintFrame(frame, known.url);
          continue;
        }
        if (known?.failed) {
          frame.dataset.state = "failed";
          continue;
        }
        frame.dataset.state = "loading";
        loadThumb(path).then((url) => paintPath(path, url));
      }
    },
    { rootMargin: "320px 0px" },
  );
  return thumbObserver;
}

// Ask for the pictures of frames that are near the viewport, and reuse every
// URL already decoded. Re-rendering the wall does not re-request pixels.
export function paintThumbnails(scope) {
  const frames = scope.querySelectorAll(".tile-frame");
  if (!frames.length) return;
  const observer = thumbObserverFor();
  for (const frame of frames) {
    const known = thumbCache.get(frame.dataset.path);
    if (known?.url) paintFrame(frame, known.url);
    else if (known?.failed) frame.dataset.state = "failed";
    else observer.observe(frame);
  }
}

export function aspectOf(row) {
  const width = Number(row?.width);
  const height = Number(row?.height);
  return width > 0 && height > 0 ? width / height : null;
}

export function pictureTile(row, { index, reason = "" } = {}) {
  const button = el("button", "tile");
  button.type = "button";
  button.id = `result-${index}`;
  button.dataset.index = String(index);
  button.dataset.path = row.path;
  button.setAttribute("role", "option");
  button.setAttribute("aria-selected", "false");
  button.tabIndex = -1;

  const frame = el("span", "tile-frame");
  frame.dataset.path = row.path;
  frame.dataset.state = "idle";

  const fallback = el("span", "tile-fallback");
  fallback.append(kindGlyph("image"));
  fallback.append(el("span", "tile-fallback-name", row.name));
  frame.append(fallback);

  if (row.marks?.length) frame.append(markLayer(row.marks));

  const caption = el("span", "tile-caption");
  caption.append(el("span", "tile-name", row.name));
  if (row.folder) caption.append(el("span", "tile-folder", row.folder));
  if (reason) caption.append(el("span", "tile-reason", reason));
  frame.append(caption);

  button.append(frame);
  return button;
}

// `rows` are laid out at their true aspect ratio: a full row takes the width it
// is given, the last row keeps the target height and stays left aligned.
export function renderWall(container, rows, { startIndex = 0, reasonOf = null } = {}) {
  const style = getComputedStyle(container);
  const width =
    container.clientWidth -
    (parseFloat(style.paddingLeft) || 0) -
    (parseFloat(style.paddingRight) || 0);
  const items = rows.map((row) => ({ row, aspect: aspectOf(row) }));
  container.replaceChildren();
  layoutRows(items, width, 168, 6).forEach((line) => {
    const rowNode = el("div", "wall-row");
    rowNode.setAttribute("role", "presentation");
    line.items.forEach(({ item, width: tileWidth, height }) => {
      const index = startIndex + rows.indexOf(item.row);
      const button = pictureTile(item.row, {
        index,
        reason: reasonOf ? reasonOf(item.row) : "",
      });
      button.style.width = `${tileWidth}px`;
      button.style.height = `${height}px`;
      rowNode.append(button);
    });
    container.append(rowNode);
  });
  paintThumbnails(container);
}

// -- documents --------------------------------------------------------------

export function documentRow(
  row,
  { index, reason = "", snippet = false, time = false } = {},
) {
  const button = el("button", "doc-row");
  button.type = "button";
  button.id = `result-${index}`;
  button.dataset.index = String(index);
  button.dataset.path = row.path;
  button.setAttribute("role", "option");
  button.setAttribute("aria-selected", "false");
  button.tabIndex = -1;

  button.append(kindGlyph(row.kind));

  const main = el("span", "doc-main");
  const head = el("span", "doc-head");
  head.append(el("span", "doc-name", row.name));
  if (row.page) head.append(el("span", "doc-page numeric", `page ${row.page}`));
  main.append(head);

  if (snippet && row.snippet?.length) {
    const text = el("span", "doc-snippet");
    appendSegments(text, row.snippet);
    main.append(text);
  }

  const sub = el("span", "doc-sub");
  if (row.folder) sub.append(el("span", "doc-folder", row.folder));
  if (reason) sub.append(el("span", "doc-reason", reason));
  if (time) {
    const when = el("time", "doc-time");
    const at = new Date(Number(row.mtime) / 1e6);
    if (!Number.isNaN(at.getTime())) {
      when.dateTime = at.toISOString();
      when.title = at.toLocaleString();
    }
    when.textContent = relativeTime(row.mtime);
    sub.append(when);
  }
  main.append(sub);
  button.append(main);
  return button;
}

export function renderDocumentList(
  container,
  rows,
  { startIndex = 0, reasonOf = null, snippet = false, time = false } = {},
) {
  container.replaceChildren();
  rows.forEach((row, position) => {
    container.append(
      documentRow(row, {
        index: startIndex + position,
        reason: reasonOf ? reasonOf(row) : "",
        snippet,
        time,
      }),
    );
  });
}
