// Tessera's window: no framework, no bundler. The Rust side owns every answer
// and every decision worth testing; this file turns those answers into DOM and
// keeps the keyboard honest.

const invoke = window.__TAURI__.core.invoke;

const state = {
  view: "search",
  query: "",
  results: [],
  selected: -1,
  // all | image | document. The filter decides which rows are visible; the
  // selection index always refers to that visible list, never to the payload.
  filter: "all",
  searchSeq: 0,
  previewSeq: 0,
  previewPath: null,
  previewUrl: null,
  paused: false,
  folders: [],
  // The empty-query state answers from the index, not from invented examples:
  // the newest files it has read, and how much there is. `firstRunChecked`
  // marks the slow status answer that can tell "no folders yet" from a pending
  // check; a `recent` payload with roots already proves an index exists.
  recent: { rows: [], indexed: 0, folders: 0 },
  recentError: null,
  homeSeq: 0,
  firstRunChecked: false,
  searchTimer: null,
  pendingTimer: null,
  pollTimer: null,
  toastTimer: null,
  // The query an in-flight search is computing, if any. The engine call
  // cannot be aborted from here, so the honest mitigation for rapid typing
  // is to never start the same search twice and to render only the newest
  // answer (the seq guard below); a superseded answer's cost is paid once.
  inflightQuery: null,
};

const $ = (id) => document.getElementById(id);

// ---- thumbnails ------------------------------------------------------------
//
// Image hits show the picture itself. The engine owns the pixels (a 320 px
// JPEG per file version); this side owns when to ask. Three rules keep the
// list honest and cheap:
//   - never on the search path: rows render from the payload first, and only
//     a row near the viewport asks for its thumbnail;
//   - one request per path, shared by every row that names it (and by a
//     re-render while it is in flight);
//   - a bounded cache of object URLs, so a long idle session does not leak.
const THUMB_CACHE_LIMIT = 160;
const thumbCache = new Map(); // path -> { url } | { failed: true }
const thumbInflight = new Map(); // path -> Promise<string | null>
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

function requestThumb(path) {
  const pending = thumbInflight.get(path);
  if (pending) return pending;
  const promise = call("thumbnail", { path })
    .then((bytes) => {
      const url = URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" }));
      rememberThumb(path, { url });
      return url;
    })
    .catch((error) => {
      // A row whose picture cannot be made keeps its kind glyph and stays
      // useful: the name, folder and text are the search answer.
      console.debug("thumbnail unavailable", path, error);
      rememberThumb(path, { failed: true });
      return null;
    })
    .finally(() => thumbInflight.delete(path));
  thumbInflight.set(path, promise);
  return promise;
}

// ---- toast -----------------------------------------------------------------

function toast(message, kind = "info") {
  const el = $("toast");
  el.textContent = message;
  el.dataset.kind = kind;
  el.hidden = false;
  // Commit the from-state styles before adding the class, so the transition
  // has two states to move between. (A rAF here would be throttled in an
  // occluded window; a reflow read is not.)
  void el.offsetWidth;
  el.classList.add("shown");
  clearTimeout(state.toastTimer);
  state.toastTimer = setTimeout(() => {
    el.classList.remove("shown");
    state.toastTimer = setTimeout(() => {
      el.hidden = true;
    }, 180);
  }, 4200);
}

function fail(context, error) {
  toast(`${context}: ${error}`, "error");
  console.error(context, error);
}

async function call(command, args = {}) {
  return invoke(command, args);
}

// ---- views -----------------------------------------------------------------

function showView(view) {
  state.view = view;
  $("app").dataset.view = view;
  // The preview belongs to a search answer; carrying it into Activity or
  // Settings leaves a pane answering a question this view no longer asks.
  if (view !== "search") closePreview();
  for (const section of ["search", "activity", "settings"]) {
    $(`view-${section}`).hidden = section !== view;
  }
  for (const button of document.querySelectorAll(".rail-item")) {
    if (button.dataset.view === view) {
      button.setAttribute("aria-current", "page");
    } else {
      button.removeAttribute("aria-current");
    }
  }
  if (view === "activity") {
    refreshActivity();
    startPolling();
  } else if (view === "settings") {
    refreshSettings();
    startPolling();
  } else {
    stopPolling();
    updateHomeVisibility();
    if (!$("home").hidden) refreshHome(true);
    $("query").focus();
  }
}

function startPolling() {
  stopPolling();
  state.pollTimer = setInterval(() => {
    if (state.view === "activity") refreshActivity(true);
    if (state.view === "settings") refreshSettings(true);
  }, 1800);
}

function stopPolling() {
  if (state.pollTimer) {
    clearInterval(state.pollTimer);
    state.pollTimer = null;
  }
}

// ---- search ----------------------------------------------------------------

function scheduleSearch() {
  clearTimeout(state.searchTimer);
  state.searchTimer = setTimeout(runSearch, 140);
}

// A first search in a session loads and verifies the on-device models inside
// the call, which can take seconds in a debug build. Without this the window
// just sat on the previous state, which reads as a miss. The text appears only
// if the call outlives the debounce (a warm search finishes first), and the
// second line names the real reason for the wait instead of spinning at
// "Searching" for ten seconds.
function beginSearchPending(seq) {
  clearTimeout(state.pendingTimer);
  state.pendingTimer = setTimeout(() => {
    if (state.searchSeq !== seq) return;
    renderPendingMeta("Searching\u2026");
    state.pendingTimer = setTimeout(() => {
      if (state.searchSeq !== seq) return;
      renderPendingMeta(
        "Loading the on-device models. The first search in a session takes longer.",
      );
    }, 1200);
  }, 160);
}

function clearSearchPending() {
  clearTimeout(state.pendingTimer);
  state.pendingTimer = null;
}

function renderPendingMeta(text) {
  const meta = $("search-meta");
  const span = document.createElement("span");
  span.className = "pending";
  span.textContent = text;
  meta.replaceChildren(span);
}

async function runSearch() {
  const query = $("query").value.trim();
  state.query = query;
  if (!query) {
    // Bump the sequence: a search still in flight for the cleared query must
    // not come back from the dead and render after the user hit Esc.
    state.searchSeq++;
    clearSearchPending();
    state.results = [];
    state.selected = -1;
    state.inflightQuery = null;
    renderResults();
    renderSearchMeta(null);
    $("notes").replaceChildren();
    closePreview();
    checkFirstRun();
    return;
  }
  if (query === state.inflightQuery) return;
  const seq = ++state.searchSeq;
  state.inflightQuery = query;
  // The home panel answers "nothing typed yet". A cold search loads the models
  // for seconds, and leaving the panel up under the pending line reads as if
  // the keystroke did nothing.
  $("home").hidden = true;
  $("empty").hidden = true;
  beginSearchPending(seq);
  let payload;
  try {
    payload = await call("search", { query, limit: 40 });
  } catch (error) {
    if (seq !== state.searchSeq) return;
    state.inflightQuery = null;
    clearSearchPending();
    renderNotes([{ text: String(error), error: true }]);
    state.results = [];
    state.selected = -1;
    renderSearchMeta(null);
    renderResults();
    return;
  }
  if (seq !== state.searchSeq) return;
  state.inflightQuery = null;
  clearSearchPending();
  state.results = payload.rows ?? [];
  // A filter that would now show nothing falls back to All rather than
  // leaving an empty panel under an active pill.
  if (state.filter !== "all" && visibleRows().length === 0) state.filter = "all";
  state.selected = visibleRows().length ? 0 : -1;
  renderNotes((payload.notes ?? []).map((text) => ({ text, error: false })));
  renderResults();
  renderSearchMeta(payload);
}

// The readout under the search bar: how long the whole wait was, the engine's
// share of it, and the arms that answered. Plain numbers the user can check —
// the honesty promise made visible. Inside the input it fought the query for
// space; below it, it is a quiet status line.
function formatMillis(ms) {
  if (!Number.isFinite(ms)) return "";
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${Math.round(ms)} ms`;
}

function renderSearchMeta(payload) {
  const meta = $("search-meta");
  if (!payload || !payload.rows?.length) {
    meta.replaceChildren();
    return;
  }
  meta.replaceChildren();
  const parts = [
    `${payload.rows.length} ${payload.rows.length === 1 ? "result" : "results"}`,
    `${formatMillis(payload.millis)} total`,
  ];
  if (Number.isFinite(payload.engine_millis)) parts.push(`${formatMillis(payload.engine_millis)} engine`);
  if (payload.arms?.length) parts.push(payload.arms.join(" + "));
  parts.forEach((text, index) => {
    if (index > 0) {
      const dot = document.createElement("span");
      dot.className = "sep";
      dot.setAttribute("aria-hidden", "true");
      dot.textContent = "·";
      meta.append(dot);
    }
    const span = document.createElement("span");
    span.textContent = text;
    meta.append(span);
  });
}

function renderNotes(notes) {
  const box = $("notes");
  box.replaceChildren();
  for (const note of notes) {
    const div = document.createElement("div");
    div.className = note.error ? "note error" : "note";
    div.textContent = note.text;
    box.append(div);
  }
}

// One glyph per file family, drawn in the same 1.5px stroke as the search
// icon. Scanning forty rows by extension string is slow; scanning them by
// shape is not.
const KIND_ICONS = {
  image:
    '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2.75" y="3.75" width="14.5" height="12.5" rx="2"></rect><circle cx="7.2" cy="8" r="1.4"></circle><path d="M3.5 14.5 8 10l3 2.75 3-3.25 2.5 2.5"></path></svg>',
  pdf:
    '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 2.75h6.5l3.5 3.5v11H5z"></path><path d="M11.5 2.75v3.5h3.5"></path><path d="M7.5 11.25h5M7.5 13.75h5"></path></svg>',
  doc: '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 2.75h6.5l3.5 3.5v11H5z"></path><path d="M11.5 2.75v3.5h3.5"></path></svg>',
  text: '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 5.5h12M4 10h12M4 14.5h8"></path></svg>',
};

function kindGroup(kind) {
  const name = String(kind || "").toLowerCase();
  // The engine's own vocabulary is image/pdf/docx/text; the extension set
  // keeps a payload from an older shape (or the dev mock) honest.
  if (name === "image" || IMAGE_EXTENSIONS.has(name)) return "image";
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

function isImageRow(row) {
  return kindGroup(row.kind) === "image";
}

function kindGlyph(kind) {
  const icon = document.createElement("span");
  icon.className = "result-icon";
  icon.innerHTML = KIND_ICONS[kindGroup(kind)];
  return icon;
}

// The picture frame a tile leads with. The glyph underneath is both the
// skeleton (dimmed while the picture loads) and the fallback (full strength
// when it cannot be shown), so a failed or missing picture never leaves a
// hole in the grid.
function thumbTile(row, className) {
  const tile = document.createElement("span");
  tile.className = className;
  tile.dataset.path = row.path;
  tile.dataset.state = "idle";
  tile.innerHTML = KIND_ICONS.image;
  return tile;
}

function paintThumb(tile, url) {
  if (!url) {
    tile.dataset.state = "failed";
    return;
  }
  let img = tile.querySelector("img");
  if (!img) {
    img = document.createElement("img");
    img.alt = "";
    img.decoding = "async";
    img.draggable = false;
    img.addEventListener(
      "load",
      () => {
        if (tile.isConnected) tile.dataset.state = "ready";
      },
      { once: true },
    );
    img.addEventListener("error", () => {
      if (tile.isConnected) tile.dataset.state = "failed";
    });
    tile.prepend(img);
  }
  if (img.dataset.url === url) {
    if (img.complete && img.naturalWidth > 0) tile.dataset.state = "ready";
    return;
  }
  img.dataset.url = url;
  img.src = url;
}

function paintThumbsFor(path) {
  const entry = thumbCache.get(path);
  for (const tile of document.querySelectorAll(".thumb")) {
    if (tile.dataset.path !== path) continue;
    if (entry?.url) paintThumb(tile, entry.url);
    else tile.dataset.state = "failed";
  }
}

// Ask for the pictures of the rows the user can actually reach. The observer
// is recreated per render (the list is rebuilt per keystroke) but the URL
// cache and in-flight map survive, so re-rendering does not re-request.
function attachThumbnails() {
  const list = $("results");
  const tiles = list.querySelectorAll(".thumb");
  if (thumbObserver) thumbObserver.disconnect();
  if (!tiles.length) return;
  if (!thumbObserver) {
    thumbObserver = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          const tile = entry.target;
          thumbObserver.unobserve(tile);
          const path = tile.dataset.path;
          if (!path) continue;
          const known = thumbCache.get(path);
          if (known?.url) {
            paintThumb(tile, known.url);
            continue;
          }
          if (known?.failed) {
            tile.dataset.state = "failed";
            continue;
          }
          tile.dataset.state = "loading";
          requestThumb(path).then(() => paintThumbsFor(path));
        }
      },
      { root: list, rootMargin: "280px 0px" },
    );
  }
  for (const tile of tiles) {
    const known = thumbCache.get(tile.dataset.path);
    if (known?.url) paintThumb(tile, known.url);
    else if (known?.failed) tile.dataset.state = "failed";
    else thumbObserver.observe(tile);
  }
}

// The visible answer: the filter applied, with images ahead of documents in
// All. The engine ranks one fused list; the split keeps each kind in that
// order, so the first image is still the strongest image match.
function visibleRows() {
  const images = state.results.filter(isImageRow);
  if (state.filter === "image") return images;
  const documents = state.results.filter((row) => !isImageRow(row));
  if (state.filter === "document") return documents;
  return images.concat(documents);
}

function visibleImageCount() {
  if (state.filter === "document") return 0;
  const rows = visibleRows();
  let count = 0;
  while (count < rows.length && isImageRow(rows[count])) count++;
  return count;
}

// The gallery is the results grid itself: tiles take the columns and document
// rows span all of them, so one scroller holds both shapes and the keyboard
// still walks a single sequence.
function galleryColumns() {
  const list = $("results");
  if (!list.classList.contains("gallery")) return 1;
  const columns = getComputedStyle(list).gridTemplateColumns;
  return columns ? columns.split(/\s+/).filter(Boolean).length : 1;
}

function renderFilters() {
  const bar = $("filters");
  bar.hidden = state.results.length === 0;
  if (bar.hidden) return;
  const images = state.results.filter(isImageRow).length;
  const counts = {
    all: state.results.length,
    image: images,
    document: state.results.length - images,
  };
  // A count of zero is a fact, not a teaser: the button says so and cannot be
  // pressed into an empty panel.
  for (const button of bar.children) {
    const count = counts[button.dataset.filter] ?? 0;
    button.querySelector(".filter-count").textContent = String(count);
    button.disabled = count === 0;
    button.setAttribute("aria-pressed", button.dataset.filter === state.filter ? "true" : "false");
  }
}

function setFilter(filter) {
  if (filter === state.filter) return;
  state.filter = filter;
  state.selected = visibleRows().length ? 0 : -1;
  closePreview();
  renderResults();
}

function resultButton(row, index, className) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = className;
  button.id = `result-${index}`;
  button.setAttribute("role", "option");
  button.setAttribute("aria-selected", index === state.selected ? "true" : "false");
  button.dataset.index = String(index);
  button.dataset.kind = isImageRow(row) ? "image" : "document";
  // Roving tabindex: the list is one tab stop, not forty.
  button.tabIndex = index === (state.selected >= 0 ? state.selected : 0) ? 0 : -1;
  button.addEventListener("click", () => select(index, true));
  return button;
}

// An image hit is the picture itself: a tile in the gallery. The name stays as
// a one-line caption so two screenshots of the same window can be told apart;
// the matched text belongs to the preview, not to the grid.
function buildTile(row, index, best) {
  const button = resultButton(row, index, "tile");
  const thumb = thumbTile(row, "thumb tile-thumb");
  if (best) {
    const badge = document.createElement("span");
    badge.className = "tile-best";
    badge.textContent = "Best match";
    thumb.append(badge);
  }
  const name = document.createElement("span");
  name.className = "tile-name";
  name.textContent = row.name;
  button.append(thumb, name);
  return button;
}

function buildResult(row, index) {
  const button = resultButton(row, index, "result");

  const lead = document.createElement("span");
  lead.className = "result-lead";
  lead.setAttribute("aria-hidden", "true");
  lead.append(kindGlyph(row.kind));

  const name = document.createElement("span");
  name.className = "result-name";
  name.textContent = row.name;

  const folder = document.createElement("span");
  folder.className = "result-folder";
  folder.textContent = row.folder;

  const badges = document.createElement("span");
  badges.className = "result-badges";
  if (row.page) {
    const page = document.createElement("span");
    page.className = "badge";
    page.textContent = `p. ${row.page}`;
    badges.append(page);
  }
  for (const arm of row.arms ?? []) {
    const badge = document.createElement("span");
    badge.className = "badge arm";
    badge.textContent = arm;
    badges.append(badge);
  }
  if (!row.arms?.length) {
    const badge = document.createElement("span");
    badge.className = "badge";
    badge.textContent = row.kind;
    badges.append(badge);
  }

  const snippet = document.createElement("span");
  snippet.className = "result-snippet";
  appendSegments(snippet, row.snippet);

  button.append(lead, name, folder, badges, snippet);
  return button;
}

function renderResults() {
  const list = $("results");
  list.replaceChildren();
  if (!state.results.length) state.filter = "all";
  const rows = visibleRows();
  const showEmpty = Boolean(state.query) && state.results.length === 0 && $("first-run").hidden;
  $("empty").hidden = !showEmpty;
  if (showEmpty) {
    const query = state.query.length > 60 ? `${state.query.slice(0, 57)}…` : state.query;
    $("empty-title").textContent = `No matches for “${query}”`;
  }
  updateHomeVisibility();
  list.setAttribute("aria-activedescendant", "");
  $("query").setAttribute("aria-activedescendant", "");
  renderFilters();
  list.classList.toggle("gallery", rows.some(isImageRow));
  rows.forEach((row, index) => {
    list.append(isImageRow(row) ? buildTile(row, index, index === 0) : buildResult(row, index));
  });
  if (state.selected >= 0 && state.selected < rows.length) {
    list.setAttribute("aria-activedescendant", `result-${state.selected}`);
    $("query").setAttribute("aria-activedescendant", `result-${state.selected}`);
  }
  attachThumbnails();
}

function appendSegments(target, segments) {
  for (const segment of segments ?? []) {
    if (segment.hit) {
      const mark = document.createElement("mark");
      mark.textContent = segment.text;
      target.append(mark);
    } else {
      target.append(document.createTextNode(segment.text));
    }
  }
}

// ---- home ------------------------------------------------------------------

// The empty-query state. No example queries and no marketing: the newest
// files the index has read, opening the same preview a search hit opens. The
// order is the file's own mtime, not `indexed_at` (a rescanned folder rewrites
// that on every unchanged file and would shuffle the list for no reason).
const HOME_LIMIT = 8;

function relativeTime(mtimeNs) {
  const then = mtimeNs / 1e6;
  if (!Number.isFinite(then)) return "";
  const seconds = Math.max(0, (Date.now() - then) / 1000);
  if (seconds < 60) return "now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
  if (seconds < 604800) return `${Math.floor(seconds / 86400)}d`;
  return new Date(then).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

async function refreshHome(quiet = false) {
  const seq = ++state.homeSeq;
  let payload;
  try {
    payload = await call("recent", { limit: HOME_LIMIT });
  } catch (error) {
    if (seq !== state.homeSeq) return;
    state.recentError = String(error);
    renderHome();
    updateHomeVisibility();
    if (!quiet) console.debug("recent files unavailable", error);
    return;
  }
  if (seq !== state.homeSeq) return;
  state.recentError = null;
  state.recent = payload;
  renderHome();
  updateHomeVisibility();
}

function renderHome() {
  const { rows, indexed, folders } = state.recent;
  const files = `${indexed.toLocaleString()} ${indexed === 1 ? "file" : "files"}`;
  $("home-counts").textContent = folders
    ? `${files} · ${folders} ${folders === 1 ? "folder" : "folders"}`
    : "";

  const note = $("home-note");
  if (state.recentError) {
    note.textContent = `The index could not be read: ${state.recentError}`;
  } else if (!rows.length) {
    note.textContent = folders
      ? "Nothing indexed yet. Files appear here as the index reads them."
      : "";
  } else {
    note.textContent = "";
  }
  note.hidden = !note.textContent;

  const list = $("home-files");
  const signature = JSON.stringify([indexed, folders, rows.map((row) => [row.path, row.mtime])]);
  if (list.dataset.signature === signature) {
    updateHomeTimes();
    attachHomeThumbs();
    return;
  }
  // Polls re-render only when the data changed; when it does, focus stays
  // with its row instead of falling back to the document.
  const focusedPath = document.activeElement?.closest?.(".home-file")?.dataset.path;
  list.dataset.signature = signature;
  list.replaceChildren();
  for (const row of rows) {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.className = "home-file";
    button.dataset.path = row.path;
    button.dataset.mtime = String(row.mtime);

    if (kindGroup(row.kind) === "image") {
      button.append(thumbTile(row, "thumb home-thumb"));
    } else {
      const lead = document.createElement("span");
      lead.className = "home-lead";
      lead.setAttribute("aria-hidden", "true");
      lead.append(kindGlyph(row.kind));
      button.append(lead);
    }

    const name = document.createElement("span");
    name.className = "home-name";
    name.textContent = row.name;

    const folder = document.createElement("span");
    folder.className = "home-folder";
    folder.textContent = row.folder;

    const when = document.createElement("time");
    when.className = "home-when";
    const at = new Date(row.mtime / 1e6);
    if (!Number.isNaN(at.getTime())) {
      when.dateTime = at.toISOString();
      when.title = at.toLocaleString();
    }
    when.textContent = relativeTime(row.mtime);

    button.append(name, folder, when);
    button.addEventListener("click", () => openPreview(row));
    item.append(button);
    list.append(item);
  }
  attachHomeThumbs();
  if (focusedPath) {
    for (const row of list.querySelectorAll(".home-file")) {
      if (row.dataset.path === focusedPath) {
        row.focus();
        break;
      }
    }
  }
}

// The relative labels age between data changes; the signature-skip path
// refreshes them without rebuilding the rows.
function updateHomeTimes() {
  for (const row of $("home-files").querySelectorAll(".home-file")) {
    const when = row.querySelector(".home-when");
    if (when) when.textContent = relativeTime(Number(row.dataset.mtime));
  }
}

// Eight rows, one request each, shared with any result row naming the same
// file. The observer that gates result thumbnails is rooted at #results, so
// the home list asks directly rather than opening a second observer.
function attachHomeThumbs() {
  for (const tile of $("home-files").querySelectorAll(".home-thumb")) {
    const path = tile.dataset.path;
    const known = thumbCache.get(path);
    if (known?.url) {
      paintThumb(tile, known.url);
      continue;
    }
    if (known?.failed) {
      tile.dataset.state = "failed";
      continue;
    }
    tile.dataset.state = "loading";
    requestThumb(path).then(() => paintThumbsFor(path));
  }
}

function updateHomeVisibility() {
  // `status` verifies the model files and can spend a minute hashing them on a
  // cold start. The home list only needs the index, so once `recent` reports a
  // root it can stand on its own and the first-run screen stays a no-roots
  // answer rather than a loading placeholder.
  const known = state.firstRunChecked || state.recent.folders > 0;
  const visible =
    known &&
    state.view === "search" &&
    !state.query &&
    state.results.length === 0 &&
    $("first-run").hidden;
  $("home").hidden = !visible;
}

function homeRows() {
  return Array.from($("home-files").querySelectorAll(".home-file"));
}

// The empty query is still a launcher: ArrowDown walks into the latest files
// without a Tab, and Escape returns to the box.
function moveHome(delta) {
  const rows = homeRows();
  if (!rows.length) return;
  const at = rows.indexOf(document.activeElement);
  const next =
    at < 0
      ? delta > 0
        ? 0
        : rows.length - 1
      : Math.min(Math.max(at + delta, 0), rows.length - 1);
  rows[next].focus();
  rows[next].scrollIntoView({ block: "nearest" });
}

function selectionFocused() {
  return Boolean(document.activeElement?.matches?.(".result, .tile"));
}

function select(index, open) {
  const rows = visibleRows();
  if (index < 0 || index >= rows.length) return;
  state.selected = index;
  for (const button of $("results").children) {
    const selected = button.dataset.index === String(index);
    button.setAttribute("aria-selected", selected ? "true" : "false");
    button.tabIndex = selected ? 0 : -1;
  }
  $("results").setAttribute("aria-activedescendant", `result-${index}`);
  $("query").setAttribute("aria-activedescendant", `result-${index}`);
  if (open) openPreview(rows[index]);
  $(`result-${index}`)?.scrollIntoView({ block: "nearest" });
}

// If focus already sits on a result, it travels with the selection. Without
// this, the next Enter would activate the row the user just left.
function advance(index) {
  const keyboard = selectionFocused();
  select(index, true);
  if (keyboard) $(`result-${index}`)?.focus();
}

function moveHorizontal(delta) {
  const rows = visibleRows();
  if (!rows.length) return;
  const from = state.selected < 0 ? 0 : state.selected;
  advance(Math.min(Math.max(from + delta, 0), rows.length - 1));
}

// Down and up follow the grid: a column step inside the image tiles, a single
// step through the document rows. Crossing between the two lands on the
// nearest edge instead of skipping the section.
function moveVertical(delta) {
  const rows = visibleRows();
  if (!rows.length) return;
  const columns = galleryColumns();
  const images = visibleImageCount();
  const from = state.selected < 0 ? (delta > 0 ? 0 : rows.length - 1) : state.selected;
  let next = from + delta;
  if (from < images && columns > 1) {
    const target = from + delta * columns;
    if (target >= 0 && target < images) {
      next = target;
    } else if (delta > 0 && images < rows.length) {
      next = images;
    } else if (delta > 0) {
      // Last image row, same column where it exists.
      next = images - 1 - ((images - 1 - (from % columns)) % columns);
    } else {
      next = Math.min(from % columns, images - 1);
    }
  } else if (delta < 0 && images > 0 && from === images) {
    next = images - 1;
  }
  advance(Math.min(Math.max(next, 0), rows.length - 1));
}

function moveToEdge(last) {
  const rows = visibleRows();
  if (!rows.length) return;
  advance(last ? rows.length - 1 : 0);
}

// ---- preview ---------------------------------------------------------------

async function openPreview(row) {
  if (!row) return;
  const seq = ++state.previewSeq;
  $("app").classList.add("preview-open");
  $("preview").hidden = false;
  $("preview-name").textContent = row.name;
  $("preview-path").textContent = row.path;
  $("preview-meta").textContent = [row.kind, row.page ? `page ${row.page}` : null]
    .filter(Boolean)
    .join(" · ");
  $("reveal").onclick = () => call("reveal", { path: row.path }).catch((e) => fail("Could not open the folder", e));

  const body = $("preview-body");
  const textBox = $("preview-text");
  textBox.replaceChildren();
  body.querySelector("img")?.remove();
  releaseImage();

  let preview;
  try {
    preview = await call("preview", { path: row.path, query: state.query });
  } catch (error) {
    if (seq !== state.previewSeq) return;
    textBox.textContent = String(error);
    return;
  }
  if (seq !== state.previewSeq) return;

  state.previewPath = preview.path;
  $("preview-name").textContent = preview.name;
  appendSegments(textBox, preview.text);
  $("preview-meta").textContent = [preview.kind, preview.page ? `page ${preview.page}` : null]
    .filter(Boolean)
    .join(" · ");

  if (preview.image_path) {
    // Put the cached thumbnail up first: it is already decoded, so the pane
    // is a picture from the first frame and sharpens when the full file
    // arrives. A preview that starts blank reads as slower than it is.
    const known = thumbCache.get(preview.path);
    const img = document.createElement("img");
    img.alt = preview.name;
    img.dataset.quality = "thumbnail";
    if (known?.url) img.src = known.url;
    body.prepend(img);
    try {
      const bytes = await call("preview_image", { path: preview.path });
      if (seq !== state.previewSeq) return;
      const url = URL.createObjectURL(new Blob([bytes]));
      state.previewUrl = url;
      img.dataset.quality = "full";
      img.src = url;
    } catch (error) {
      if (seq === state.previewSeq && !known?.url) {
        img.remove();
        textBox.textContent = `The image could not be read: ${error}`;
      }
    }
  }
}

function releaseImage() {
  if (state.previewUrl) {
    URL.revokeObjectURL(state.previewUrl);
    state.previewUrl = null;
  }
}

function closePreview() {
  state.previewSeq++;
  releaseImage();
  state.previewPath = null;
  // Drop the picture too: a hidden pane holding a full-size image is memory
  // the next search does not need.
  $("preview-body").querySelector("img")?.remove();
  $("preview").hidden = true;
  $("app").classList.remove("preview-open");
}

// ---- activity --------------------------------------------------------------

async function refreshActivity(quiet = false) {
  let status;
  let watch;
  try {
    [status, watch] = await Promise.all([call("status"), call("watch_status")]);
  } catch (error) {
    if (!quiet) fail("Could not read the index", error);
    return;
  }

  state.paused = watch.paused;
  $("watch-state").textContent = watch.running ? (watch.paused ? "paused" : "watching") : "stopped";
  $("watch-state").className = `pill ${watch.running ? (watch.paused ? "paused" : "live") : ""}`;
  $("watch-toggle").textContent = watch.paused ? "Resume indexing" : "Pause indexing";
  $("watch-toggle").disabled = !watch.running;
  $("watch-summary").textContent = watch.summary ?? (watch.running ? "Files saved now are indexed within seconds." : "");

  const log = $("watch-log");
  const logText = (watch.log ?? []).slice(-60).join("\n");
  // The pane polls every 1.8 s. Rewriting an unchanged log would re-announce
  // it to a screen reader and move the scroll position for no reason.
  if (log.textContent !== logText) {
    const atBottom = log.scrollTop + log.clientHeight >= log.scrollHeight - 8;
    log.textContent = logText;
    if (atBottom) log.scrollTop = log.scrollHeight;
  }

  const folders = $("folders");
  setIfChanged(folders, status.folders.join("\u0000"), (list) => {
    for (const folder of status.folders) {
      const li = document.createElement("li");
      li.textContent = folder;
      list.append(li);
    }
    if (!status.folders.length) {
      const li = document.createElement("li");
      li.className = "dim";
      li.textContent = "No folders indexed yet.";
      list.append(li);
    }
  });

  const facts = $("index-facts");
  const counts = Object.fromEntries(status.counts ?? []);
  const factRows = [
    ["Files indexed", counts.indexed ?? 0],
    ["Awaiting OCR", counts.awaiting_ocr ?? 0],
    ["Failed", status.failures.length],
    ["Image vectors", status.image_vectors],
    ["Text vectors", status.text_vectors],
    ["Index size", `${status.index_mb.toFixed(1)} MB`],
  ];
  setIfChanged(facts, JSON.stringify(factRows), (dl) => {
    for (const [label, value] of factRows) dl.append(term(label), detail(value));
  });

  const failures = $("failures");
  $("failure-count").textContent = String(status.failures.length);
  $("failure-empty").hidden = status.failures.length > 0;
  setIfChanged(failures, JSON.stringify(status.failures.slice(0, 40)), (list) => {
    for (const failure of status.failures.slice(0, 40)) {
      const li = document.createElement("li");
      const path = document.createElement("div");
      path.className = "path";
      path.textContent = failure.path;
      const error = document.createElement("div");
      error.className = "error";
      error.textContent = failure.error;
      li.append(path, error);
      list.append(li);
    }
  });

  const skips = $("skip-facts");
  $("skip-empty").hidden = status.skip_reasons.length > 0;
  setIfChanged(skips, JSON.stringify(status.skip_reasons), (dl) => {
    for (const [reason, count] of status.skip_reasons) dl.append(term(reason), detail(count));
  });

  $("activity-dot").hidden = status.failures.length === 0;
}

// Replace an element's children only when the data behind them changed.
//
// The Activity and Settings panes poll; rebuilding a list on every tick
// destroys focus inside it (a Download button mid-click) and re-announces
// unchanged content. Keyed by a signature, the DOM follows the data.
function setIfChanged(el, signature, build) {
  if (el.dataset.signature === signature) return;
  el.dataset.signature = signature;
  el.replaceChildren();
  build(el);
}

function term(text) {
  const dt = document.createElement("dt");
  dt.textContent = text;
  return dt;
}

function detail(value) {
  const dd = document.createElement("dd");
  dd.textContent = String(value);
  return dd;
}

// ---- settings --------------------------------------------------------------

async function refreshSettings(quiet = false) {
  let status;
  try {
    status = await call("status");
  } catch (error) {
    if (!quiet) fail("Could not read settings", error);
    return;
  }
  state.folders = status.folders;
  setIfChanged($("settings-folders"), status.folders.join("\u0000"), (list) => {
    for (const folder of status.folders) {
      const li = document.createElement("li");
      li.textContent = folder;
      list.append(li);
    }
    if (!status.folders.length) {
      const li = document.createElement("li");
      li.className = "dim";
      li.textContent = "No folders yet. Add one to begin.";
      list.append(li);
    }
  });

  const table = $("models");
  setIfChanged(table, JSON.stringify(status.models ?? []), (el) => {
    const head = document.createElement("tr");
    for (const label of ["Set", "State", ""]) {
      const th = document.createElement("th");
      th.textContent = label;
      head.append(th);
    }
    el.append(head);
    for (const set of status.models ?? []) {
      const tr = document.createElement("tr");
      const name = document.createElement("td");
      name.textContent = set.name;
      const stateCell = document.createElement("td");
      stateCell.className = set.complete ? "state" : "state missing";
      stateCell.textContent = set.complete ? "installed" : `${set.missing.length} file(s) missing`;
      const action = document.createElement("td");
      if (!set.complete) {
        const button = document.createElement("button");
        button.className = "ghost";
        button.textContent = "Download";
        button.addEventListener("click", () => installModels([set.name], button));
        action.append(button);
      }
      tr.append(name, stateCell, action);
      el.append(tr);
    }
  });

  const facts = $("data-facts");
  const dataRows = [
    ["Index", status.index_path],
    ["Index size", `${status.index_mb.toFixed(1)} MB`],
    ["Image vectors", status.image_vectors],
    ["Text vectors", status.text_vectors],
  ];
  setIfChanged(facts, JSON.stringify(dataRows), (dl) => {
    for (const [label, value] of dataRows) dl.append(term(label), detail(value));
  });

  try {
    $("autostart").checked = await call("autostart_enabled");
  } catch {
    $("autostart").checked = false;
  }
}

async function installModels(sets, button) {
  const label = button.textContent;
  button.disabled = true;
  button.textContent = "Downloading…";
  $("models-note").textContent =
    "Downloading and verifying. This is the only network access the app makes, and only because you asked.";
  try {
    const lines = await call("install_models", { sets });
    $("models-note").textContent = lines.join(" · ") || "Models installed.";
  } catch (error) {
    $("models-note").textContent = `Download failed: ${error}`;
  } finally {
    button.disabled = false;
    button.textContent = label;
    refreshSettings(true);
  }
}

async function addFolder() {
  let folder;
  try {
    folder = await call("pick_folder");
  } catch (error) {
    fail("Could not open the folder picker", error);
    return;
  }
  if (!folder) return;
  $("indexing-note").textContent = `Indexing ${folder}… searching is available while it runs.`;
  $("add-folder").disabled = true;
  try {
    const summary = await call("index_folder", { path: folder });
    $("indexing-note").textContent = summary;
    toast(summary);
    await refreshSettings(true);
    await checkFirstRun();
  } catch (error) {
    $("indexing-note").textContent = `Could not index: ${error}`;
    fail("Could not index that folder", error);
  } finally {
    $("add-folder").disabled = false;
  }
}

async function rebuildIndex() {
  let confirmed = false;
  try {
    confirmed = await call("confirm_rebuild");
  } catch (error) {
    fail("Could not ask for confirmation", error);
    return;
  }
  if (!confirmed) return;
  try {
    const folders = await call("rebuild_index");
    toast(
      folders.length
        ? `Index deleted. Rebuilding ${folders.length} folder(s) in the background.`
        : "Index deleted. Add a folder to rebuild.",
    );
    await refreshSettings(true);
    await checkFirstRun();
  } catch (error) {
    fail("Could not rebuild the index", error);
  }
}

// ---- engine state / first run ---------------------------------------------

function setEngine(label, tone) {
  $("engine-dot").className = `engine-dot ${tone}`;
  $("engine-dot").title = `Engine: ${label}`;
  // The label is a live region polled every six seconds; only a real change
  // should be announced.
  const live = $("engine-label");
  if (live.textContent !== label) live.textContent = label;
}

async function checkFirstRun() {
  let status;
  try {
    status = await call("status");
  } catch (error) {
    setEngine("engine unavailable", "bad");
    state.recentError = String(error);
    state.firstRunChecked = true;
    renderHome();
    updateHomeVisibility();
    return;
  }
  const hasFolders = (status.folders ?? []).length > 0;
  $("first-run").hidden = hasFolders;
  state.firstRunChecked = true;
  if (!hasFolders) {
    state.results = [];
    renderResults();
  } else {
    updateHomeVisibility();
    // The home list follows the watcher on the same six-second cadence as the
    // engine state, so a file saved now appears without its own timer.
    if (!$("home").hidden) refreshHome(true);
  }
  const modelsMissing = (status.models ?? []).some((set) => !set.complete);
  setEngine(
    modelsMissing ? "models need download" : hasFolders ? "ready" : "no folders yet",
    modelsMissing ? "warn" : "ok",
  );
  return status;
}

// ---- events ----------------------------------------------------------------

document.querySelectorAll(".rail-item").forEach((button) => {
  button.addEventListener("click", () => showView(button.dataset.view));
});

document.querySelectorAll(".filter").forEach((button) => {
  button.addEventListener("click", () => setFilter(button.dataset.filter));
});

$("query").addEventListener("input", () => {
  scheduleSearch();
  closePreview();
});

$("search-form").addEventListener("submit", (event) => {
  event.preventDefault();
  if (state.selected >= 0) select(state.selected, true);
});

// Arrow keys work wherever the focus is on the search view — from the input,
// from the list, from nowhere in particular. Spotlight and Raycast never ask
// the user to tab somewhere before the arrows answer; neither does this.
document.addEventListener("keydown", (event) => {
  const mod = event.ctrlKey || event.metaKey;
  if (mod && event.key.toLowerCase() === "k") {
    event.preventDefault();
    showView("search");
    $("query").select();
    return;
  }
  if (mod && event.key === "1") {
    event.preventDefault();
    showView("search");
    return;
  }
  if (mod && event.key === "2") {
    event.preventDefault();
    showView("activity");
    return;
  }
  if (mod && event.key === "3") {
    event.preventDefault();
    showView("settings");
    return;
  }
  if (event.key === "Escape") {
    if (!$("preview").hidden) {
      closePreview();
    } else if (state.view !== "search") {
      showView("search");
    } else if (document.activeElement?.closest?.("#home-files")) {
      $("query").focus();
    } else if ($("query").value) {
      $("query").value = "";
      runSearch();
    }
    return;
  }
  if (state.view !== "search") return;
  // A launcher is always ready for a query. If a home row holds focus after
  // an arrow, the next character belongs in the box, not nowhere.
  if (
    event.key.length === 1 &&
    !mod &&
    !event.altKey &&
    document.activeElement?.closest?.("#home-files")
  ) {
    $("query").focus();
    return;
  }
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    const delta = event.key === "ArrowDown" ? 1 : -1;
    if (visibleRows().length) {
      event.preventDefault();
      moveVertical(delta);
    } else if (!$("home").hidden && homeRows().length) {
      event.preventDefault();
      moveHome(delta);
    }
    return;
  }
  if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
    // Caret keys stay caret keys while the query has text to move through. An
    // empty box has no caret journey, so there they walk the gallery.
    const typing = document.activeElement === $("query") && $("query").value.length > 0;
    if (!typing && visibleRows().length) {
      event.preventDefault();
      moveHorizontal(event.key === "ArrowRight" ? 1 : -1);
    }
    return;
  }
  // Home/End belong to the caret while the query has focus; they jump the
  // list only when the user is not typing.
  if ((event.key === "Home" || event.key === "End") && document.activeElement !== $("query")) {
    if (visibleRows().length) {
      event.preventDefault();
      moveToEdge(event.key === "End");
    }
    return;
  }
  if (event.key === "Enter") {
    const target = document.activeElement;
    // Inputs and buttons handle their own Enter; only the unclaimed Enter
    // opens the selection.
    if (target?.closest?.("button, input, a")) return;
    if (state.selected >= 0) {
      event.preventDefault();
      select(state.selected, true);
    }
  }
});

$("preview-close").addEventListener("click", closePreview);
// The miss state names its cause; this is the action that follows from it.
$("empty-clear").addEventListener("click", () => {
  $("query").value = "";
  $("query").focus();
  runSearch();
});
$("watch-toggle").addEventListener("click", async () => {
  try {
    await call("set_paused", { paused: !state.paused });
    refreshActivity(true);
  } catch (error) {
    fail("Could not change the watcher", error);
  }
});
$("add-folder").addEventListener("click", addFolder);
$("first-run-add").addEventListener("click", async () => {
  showView("settings");
  await addFolder();
});
$("rebuild").addEventListener("click", rebuildIndex);
$("refresh-activity").addEventListener("click", () => refreshActivity());
$("refresh-settings").addEventListener("click", () => refreshSettings());
$("autostart").addEventListener("change", async (event) => {
  try {
    await call("set_autostart", { enabled: event.target.checked });
    toast(event.target.checked ? "Tessera will start when you sign in." : "Autostart off.");
  } catch (error) {
    event.target.checked = !event.target.checked;
    fail("Could not change autostart", error);
  }
});

// ---- boot ------------------------------------------------------------------

(async function boot() {
  renderResults();
  // Both requests go out together: `recent` is one index query, `status`
  // verifies the models. Waiting for the model check before showing the home
  // list would leave the window blank for the length of that hash.
  const firstRun = checkFirstRun();
  refreshHome();
  await firstRun;
  setInterval(checkFirstRun, 6000);
  $("query").focus();
})();
