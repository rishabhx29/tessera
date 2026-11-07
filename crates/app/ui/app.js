// Tessera's window: no framework, no bundler. The Rust side owns every answer
// and every decision worth testing; this file turns those answers into DOM and
// keeps the keyboard honest.

const invoke = window.__TAURI__.core.invoke;

const state = {
  view: "search",
  query: "",
  results: [],
  selected: -1,
  searchSeq: 0,
  previewSeq: 0,
  previewPath: null,
  previewUrl: null,
  paused: false,
  folders: [],
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
  state.selected = state.results.length ? 0 : -1;
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

// The square tile an image row leads with. The glyph underneath is both the
// skeleton (dimmed while the picture loads) and the fallback (full strength
// when it cannot be shown), so a failed or missing picture never leaves a
// hole in the row.
function thumbTile(row) {
  const tile = document.createElement("span");
  tile.className = "result-thumb";
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
  for (const tile of document.querySelectorAll(".result-thumb")) {
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
  const tiles = list.querySelectorAll(".result-thumb");
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

function renderResults() {
  const list = $("results");
  list.replaceChildren();
  const showEmpty = Boolean(state.query) && state.results.length === 0 && $("first-run").hidden;
  $("empty").hidden = !showEmpty;
  if (showEmpty) {
    const query = state.query.length > 60 ? `${state.query.slice(0, 57)}…` : state.query;
    $("empty-title").textContent = `No matches for “${query}”`;
  }
  $("home").hidden = Boolean(state.query) || state.results.length > 0 || !$("first-run").hidden;
  list.setAttribute("aria-activedescendant", "");
  $("query").setAttribute("aria-activedescendant", "");
  state.results.forEach((row, index) => {
    const image = isImageRow(row);
    const button = document.createElement("button");
    button.type = "button";
    button.className = "result";
    button.id = `result-${index}`;
    button.setAttribute("role", "option");
    button.setAttribute("aria-selected", index === state.selected ? "true" : "false");
    button.dataset.index = String(index);
    button.dataset.kind = image ? "image" : "document";
    // Roving tabindex: the list is one tab stop, not forty.
    button.tabIndex = index === (state.selected >= 0 ? state.selected : 0) ? 0 : -1;

    const lead = document.createElement("span");
    lead.className = "result-lead";
    lead.setAttribute("aria-hidden", "true");
    lead.append(image ? thumbTile(row) : kindGlyph(row.kind));

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
    button.addEventListener("click", () => select(index, true));
    list.append(button);
  });
  if (state.selected >= 0) {
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

function select(index, open) {
  if (index < 0 || index >= state.results.length) return;
  state.selected = index;
  for (const button of document.querySelectorAll(".result")) {
    const selected = button.dataset.index === String(index);
    button.setAttribute("aria-selected", selected ? "true" : "false");
    button.tabIndex = selected ? 0 : -1;
  }
  $("results").setAttribute("aria-activedescendant", `result-${index}`);
  $("query").setAttribute("aria-activedescendant", `result-${index}`);
  const row = state.results[index];
  if (open) openPreview(row);
  const active = $(`result-${index}`);
  active?.scrollIntoView({ block: "nearest" });
}

function move(delta) {
  if (!state.results.length) return;
  const from = state.selected < 0 ? 0 : state.selected;
  const next = Math.min(Math.max(from + delta, 0), state.results.length - 1);
  // If focus already sits on a row, it travels with the selection. Without
  // this, the next Enter would activate the row the user just left.
  const keyboard = document.activeElement?.classList?.contains("result");
  select(next, true);
  if (keyboard) $(`result-${next}`)?.focus();
}

function moveToEdge(last) {
  if (!state.results.length) return;
  const keyboard = document.activeElement?.classList?.contains("result");
  const next = last ? state.results.length - 1 : 0;
  select(next, true);
  if (keyboard) $(`result-${next}`)?.focus();
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

async function checkFirstRun() {
  let status;
  try {
    status = await call("status");
  } catch {
    $("engine-dot").className = "engine-dot bad";
    $("engine-label").textContent = "engine unavailable";
    return;
  }
  const hasFolders = (status.folders ?? []).length > 0;
  $("first-run").hidden = hasFolders;
  if (!hasFolders) {
    state.results = [];
    renderResults();
  } else if (!$("first-run").hidden) {
    state.results = [];
    renderResults();
  }
  const modelsMissing = (status.models ?? []).some((set) => !set.complete);
  $("engine-dot").className = `engine-dot ${modelsMissing ? "warn" : "ok"}`;
  $("engine-label").textContent = modelsMissing
    ? "models need download"
    : hasFolders
      ? "ready"
      : "no folders yet";
  return status;
}

// ---- events ----------------------------------------------------------------

document.querySelectorAll(".rail-item").forEach((button) => {
  button.addEventListener("click", () => showView(button.dataset.view));
});

// The home state's example queries are real searches, not decoration: each
// fills the box and runs, so the three arms demonstrate themselves.
document.querySelectorAll(".example").forEach((button) => {
  button.addEventListener("click", () => {
    const input = $("query");
    input.value = button.dataset.query;
    input.focus();
    runSearch();
  });
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
    } else if ($("query").value) {
      $("query").value = "";
      runSearch();
    }
    return;
  }
  if (state.view !== "search") return;
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    if (state.results.length) {
      event.preventDefault();
      move(event.key === "ArrowDown" ? 1 : -1);
    }
    return;
  }
  // Home/End belong to the caret while the query has focus; they jump the
  // list only when the user is not typing.
  if ((event.key === "Home" || event.key === "End") && document.activeElement !== $("query")) {
    if (state.results.length) {
      event.preventDefault();
      moveToEdge(event.key === "End");
    }
    return;
  }
  if (event.key === "Enter") {
    const target = document.activeElement;
    // The input's Enter is the form submit; a focused result button clicks
    // itself. Only the unclaimed Enter opens the selection.
    if (target === $("query") || target?.classList?.contains("result")) return;
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
  await checkFirstRun();
  setInterval(checkFirstRun, 6000);
  $("query").focus();
})();
