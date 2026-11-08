// Search answers: the same picture wall as home, then document rows, each
// carrying why it surfaced when the `all` lens asked every question at once.

import { LENSES } from "./lenses.js";
import {
  count,
  el,
  formatMillis,
  isImageKind,
  plural,
  reasonFor,
  renderDocumentList,
  renderWall,
} from "./dom.js";

export const SEARCH_LIMIT = 60;

let ctx = null;
let nodes = null;

export function initResults(root, context) {
  ctx = context;
  nodes = {
    wall: root.querySelector("#wall"),
    documents: root.querySelector("#doc-list"),
    note: root.querySelector("#wall-note"),
    miss: root.querySelector("#miss"),
    missTitle: root.querySelector("#miss-title"),
    missHint: root.querySelector("#miss-hint"),
    missLenses: root.querySelector("#miss-lenses"),
    meta: document.getElementById("meta"),
  };
}

export function render(payload) {
  if (!nodes) return;
  const { state } = ctx;
  const rows = state.results ?? [];
  const pictures = rows.filter((row) => isImageKind(row.kind));
  const documents = rows.filter((row) => !isImageKind(row.kind));
  state.visibleRows = pictures.concat(documents);

  const reasonOf =
    state.lens === "all" ? (row) => reasonFor(row, state.resultQuery) : null;

  nodes.wall.hidden = !pictures.length;
  nodes.documents.hidden = !documents.length;
  nodes.note.hidden = true;
  if (pictures.length) {
    renderWall(nodes.wall, pictures, { startIndex: 0, reasonOf });
  } else {
    nodes.wall.replaceChildren();
  }
  if (documents.length) {
    renderDocumentList(nodes.documents, documents, {
      startIndex: pictures.length,
      reasonOf,
      snippet: true,
    });
  } else {
    nodes.documents.replaceChildren();
  }

  renderMiss();
  renderMeta(payload);
}

export function clear() {
  if (!nodes) return;
  nodes.wall.replaceChildren();
  nodes.documents.replaceChildren();
  nodes.wall.hidden = true;
  nodes.documents.hidden = true;
  nodes.miss.hidden = true;
  nodes.meta.replaceChildren();
  ctx.state.visibleRows = [];
}

function renderMeta(payload) {
  const meta = nodes.meta;
  meta.replaceChildren();
  if (!payload) return;
  const total = payload.rows?.length ?? 0;
  const latency = formatMillis(payload.millis);
  const counted =
    total >= SEARCH_LIMIT
      ? `${count(total)} shown`
      : `${count(total)} ${plural(total, "result", "results")}`;
  meta.textContent = latency ? `${counted} · ${latency}` : counted;
  meta.title = payload.engine_millis
    ? `${formatMillis(payload.engine_millis)} in the engine`
    : "";
}

function renderMiss() {
  const { state } = ctx;
  const show =
    Boolean(state.query) && state.results.length === 0 && !state.showFirstRun;
  nodes.miss.hidden = !show;
  if (!show) return;

  const query = state.query.length > 60 ? `${state.query.slice(0, 57)}…` : state.query;
  nodes.missTitle.textContent = `nothing found for “${query}”`;

  const hint =
    state.lens === "pictures"
      ? "picture search matches what a picture looks like, not the words on it. Try the lens that reads words inside pictures."
      : "Try a shorter word, another lens, or a filter such as type:pdf.";
  nodes.missHint.textContent = hint;

  nodes.missLenses.replaceChildren();
  for (const lens of LENSES) {
    if (lens.id === state.lens) continue;
    const button = el("button", "miss-lens", lens.label);
    button.type = "button";
    button.setAttribute("aria-label", lens.aria);
    button.addEventListener("click", () => ctx.setLens(lens.id));
    nodes.missLenses.append(button);
  }

  if (!nodes.missClear) {
    nodes.missClear = el("button", "miss-clear", "clear");
    nodes.missClear.type = "button";
    nodes.missClear.addEventListener("click", () => ctx.clearQuery());
    nodes.miss.append(nodes.missClear);
  }
}

// The status line under the field while a query is still being answered.
export function showPending(text) {
  if (!nodes) return;
  nodes.meta.replaceChildren();
  nodes.meta.append(el("span", "pending", text));
}
