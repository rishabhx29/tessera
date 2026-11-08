// The home state: the newest pictures as a justified wall, or the newest
// documents as rows when the documents lens asks for them.
//
// The wall is the library, not a feed of activity. Every tile is a file the
// index holds, at its own aspect ratio, and the counts describe the whole
// index rather than the page on screen.

import { call } from "./ipc.js";
import {
  count,
  el,
  isImageKind,
  paintThumbnails,
  plural,
  renderDocumentList,
  renderWall,
} from "./dom.js";

const PICTURE_LIMIT = 400;
const DOCUMENT_LIMIT = 200;
const DOCUMENT_KINDS = ["pdf", "docx", "text"];
const PICTURE_KINDS = ["image"];

let ctx = null;
let nodes = null;
let signature = "";

export function initLibrary(root, context) {
  ctx = context;
  nodes = {
    wall: root.querySelector("#wall"),
    documents: root.querySelector("#doc-list"),
    note: root.querySelector("#wall-note"),
    meta: document.getElementById("meta"),
  };
}

export function homeRows() {
  return ctx?.state.visibleRows ?? [];
}

async function fetchLibrary(lens) {
  const documents = lens === "documents";
  return call("library", {
    kinds: documents ? DOCUMENT_KINDS : PICTURE_KINDS,
    limit: documents ? DOCUMENT_LIMIT : PICTURE_LIMIT,
  });
}

// Returns the payload on success, or null when a newer request won.
export async function refresh(quiet = false) {
  const { state } = ctx;
  const seq = ++state.librarySeq;
  const lens = state.lens;
  let payload;
  try {
    payload = await fetchLibrary(lens);
  } catch (error) {
    if (seq !== state.librarySeq || lens !== state.lens) return null;
    state.libraryError = String(error);
    render();
    if (!quiet) ctx.toast(`Could not read the library: ${error}`, "error");
    return null;
  }
  if (seq !== state.librarySeq || lens !== state.lens) return null;
  state.library = payload;
  state.libraryError = null;
  state.firstRunChecked = true;
  render();
  return payload;
}

export function render() {
  if (!nodes) return;
  const { state } = ctx;
  // The wall and the result wall share one container. While a query is on the
  // screen the home render must not repaint it underneath the results.
  if (state.query) return;
  const { lens, library } = state;
  const documents = lens === "documents";
  const rows = library.rows ?? [];
  const pictures = documents ? [] : rows.filter((row) => isImageKind(row.kind));
  const files = documents ? rows.filter((row) => !isImageKind(row.kind)) : [];

  state.visibleRows = documents ? files : pictures;

  const nextSignature = JSON.stringify([
    lens,
    library.pictures,
    library.documents,
    rows.map((row) => [row.path, row.mtime, row.width, row.height]),
  ]);
  const unchanged = nextSignature === signature;
  signature = nextSignature;

  if (!unchanged) {
    nodes.wall.hidden = documents || !pictures.length;
    nodes.documents.hidden = !documents || !files.length;
    if (!documents) {
      renderWall(nodes.wall, pictures);
    } else {
      renderDocumentList(nodes.documents, files, { time: true });
    }
  } else if (!documents) {
    paintThumbnails(nodes.wall);
  }

  renderNote({ documents, pictures, files, library });
  renderCounts(library);
}

function renderNote({ documents, pictures, files, library }) {
  const note = nodes.note;
  note.replaceChildren();
  let text = "";
  if (ctx.state.libraryError) {
    text = `The index could not be read: ${ctx.state.libraryError}`;
  } else if (documents) {
    if (!files.length) text = "documents appear here as they are read.";
    else if ((library.documents ?? 0) > files.length) {
      text = `showing the newest ${count(files.length)} documents.`;
    }
  } else if (!pictures.length) {
    text = "pictures appear here as they are read.";
  } else if (ctx.state.lens === "words") {
    text =
      "words searches the text inside pictures. type a word to find the line it appears on.";
  } else if ((library.pictures ?? 0) > pictures.length) {
    text = `showing the newest ${count(pictures.length)} pictures.`;
  }

  if (!text) {
    note.hidden = true;
    return;
  }
  note.hidden = false;
  note.append(el("span", "", text));
  if (!documents && !pictures.length && (library.documents ?? 0) > 0) {
    const more = el(
      "button",
      "",
      `${count(library.documents)} ${plural(library.documents, "document", "documents")} indexed`,
    );
    more.type = "button";
    more.addEventListener("click", () => ctx.setLens("documents"));
    note.append(more);
  }
}

function renderCounts(library) {
  const pictures = library.pictures ?? 0;
  const documents = library.documents ?? 0;
  nodes.meta.textContent = `${count(pictures)} ${plural(pictures, "picture", "pictures")} · ${count(documents)} ${plural(documents, "document", "documents")}`;
}

// The first-run answer: a folder count of zero means the card, not an empty
// wall that could be mistaken for a search miss.
export function clearSignature() {
  signature = "";
}
