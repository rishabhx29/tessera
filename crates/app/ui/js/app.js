// Tessera's window: no framework, no bundler. The Rust side owns every answer
// and every decision worth testing; this file keeps the state, schedules the
// one search box, and makes the keyboard work.

import { call, listen } from "./ipc.js";
import { el, initToast, renderNotes, toast } from "./dom.js";
import { DEFAULT_LENS, LENSES, lensFor } from "./lenses.js";
import { initTheme } from "./theme.js";
import * as library from "./library.js";
import * as results from "./results.js";
import * as preview from "./preview.js";
import * as settings from "./settings.js";

const SEARCH_DEBOUNCE_MS = 140;
const PENDING_AFTER_MS = 160;
const MODEL_NOTE_AFTER_MS = 1200;
const HOME_REFRESH_MS = 6000;

const state = {
  view: "library",
  lens: DEFAULT_LENS,
  query: "",
  // The query whose answer is on screen. The field may hold a newer string
  // while its search is still running.
  resultQuery: "",
  results: [],
  searchPayload: null,
  visibleRows: [],
  selected: -1,
  searchSeq: 0,
  searchTimer: null,
  pendingTimer: null,
  inflightQuery: null,
  librarySeq: 0,
  library: { rows: [], pictures: 0, documents: 0, indexed: 0, folders: 0 },
  libraryError: null,
  firstRunChecked: false,
  showFirstRun: false,
  pollTimer: null,
  resultsWidth: 0,
};

const $ = (id) => document.getElementById(id);

let ctx = null;

// ---- boot -----------------------------------------------------------------

initTheme();
initToast($("toast"));
buildLenses();

ctx = {
  state,
  call,
  setLens,
  openPreview,
  refreshLibrary,
  toast,
  clearQuery,
  showLibrary,
  showSettings,
};

library.initLibrary($("view-library"), ctx);
results.initResults($("view-library"), ctx);
preview.initPreview($("preview"), ctx);
settings.initSettings($("view-settings"), ctx);

bindEvents();
listen("tessera-show", resetToHome).catch(() => {});

$("query").focus();
refreshLibrary(true);
startHomePolling();

// The window is created hidden and revealed once the page has something to
// paint, so the first frame is Tessera, not an empty rectangle.
call("boot").catch(() => {});

// ---- header ---------------------------------------------------------------

function buildLenses() {
  const group = $("lenses");
  LENSES.forEach((lens, position) => {
    const button = el("button", "lens", lens.label);
    button.type = "button";
    button.dataset.lens = lens.id;
    button.setAttribute("aria-label", lens.aria);
    button.setAttribute("aria-keyshortcuts", `Control+${position + 1}`);
    button.addEventListener("click", () => setLens(lens.id));
    group.append(button);
  });
  paintLenses();
  $("query").placeholder = lensFor(state.lens).placeholder;
}

function paintLenses() {
  $("app").dataset.lens = state.lens;
  for (const button of $("lenses").querySelectorAll("[data-lens]")) {
    button.setAttribute("aria-pressed", button.dataset.lens === state.lens ? "true" : "false");
  }
}

function setLens(id) {
  const lens = lensFor(id);
  if (state.view === "library" && state.lens === lens.id && !state.query) return;
  state.lens = lens.id;
  paintLenses();
  $("query").placeholder = lens.placeholder;
  closePreview();
  if (state.view !== "library") showLibrary();

  if (!state.query) {
    state.selected = -1;
    state.searchPayload = null;
    results.clear();
    library.clearSignature();
    library.render();
    updateHomeVisibility();
    paintSelection();
    refreshLibrary(true);
    return;
  }
  resetResults();
  runSearch(true);
}

// ---- search ---------------------------------------------------------------

function scheduleSearch() {
  clearTimeout(state.searchTimer);
  state.searchTimer = setTimeout(runSearch, SEARCH_DEBOUNCE_MS);
}

async function runSearch(force = false) {
  const query = $("query").value.trim();
  state.query = query;
  syncClear();

  if (!query) {
    // Bump the sequence: an answer for the cleared query must not arrive and
    // paint a wall the user has already left.
    state.searchSeq++;
    state.inflightQuery = null;
    clearPending();
    resetResults();
    $("notes").replaceChildren();
    closePreview();
    library.clearSignature();
    library.render();
    updateHomeVisibility();
    paintSelection();
    refreshLibrary(true);
    return;
  }

  if (!force && query === state.inflightQuery) return;
  const seq = ++state.searchSeq;
  state.inflightQuery = query;
  beginPending(seq);
  $("miss").hidden = true;
  $("first-run").hidden = true;

  let payload;
  try {
    payload = await call("search", { query, mode: state.lens, limit: results.SEARCH_LIMIT });
  } catch (error) {
    if (seq !== state.searchSeq) return;
    state.inflightQuery = null;
    clearPending();
    // The previous answer stays on screen; one quiet line explains the error.
    renderNotes($("notes"), [{ text: String(error), error: true }]);
    return;
  }
  if (seq !== state.searchSeq) return;
  state.inflightQuery = null;
  clearPending();
  state.resultQuery = query;
  state.results = payload.rows ?? [];
  state.searchPayload = payload;
  state.selected = state.results.length ? 0 : -1;
  renderNotes($("notes"), payload.notes ?? []);
  results.render(payload);
  paintSelection();
  updateHomeVisibility();
}

function beginPending(seq) {
  clearPending();
  state.pendingTimer = setTimeout(() => {
    if (state.searchSeq !== seq) return;
    results.showPending("searching…");
    state.pendingTimer = setTimeout(() => {
      if (state.searchSeq !== seq) return;
      results.showPending(
        "Loading the on-device models. The first search in a session takes longer.",
      );
    }, MODEL_NOTE_AFTER_MS);
  }, PENDING_AFTER_MS);
}

function clearPending() {
  clearTimeout(state.pendingTimer);
  state.pendingTimer = null;
}

function resetResults() {
  state.results = [];
  state.searchPayload = null;
  state.resultQuery = "";
  state.selected = -1;
  state.visibleRows = [];
  results.clear();
  $("miss").hidden = true;
}

function clearQuery() {
  $("query").value = "";
  runSearch();
  $("query").focus();
}

function syncClear() {
  $("clear").hidden = !$("query").value;
}

// ---- home -----------------------------------------------------------------

async function refreshLibrary(quiet = false) {
  const payload = await library.refresh(quiet);
  if (payload) {
    updateHomeVisibility();
    if (!state.query) paintSelection();
  }
  return payload;
}

function updateHomeVisibility() {
  const home = state.view === "library" && !state.query;
  const first =
    home &&
    state.firstRunChecked &&
    (state.library.folders ?? 0) === 0 &&
    !state.libraryError;
  state.showFirstRun = first;
  $("first-run").hidden = !first;
  $("library-results").hidden = first;
  $("query").setAttribute("aria-expanded", String(!first && Boolean(state.query)));
  if (first) $("wall-note").hidden = true;
}

function startHomePolling() {
  if (state.pollTimer) clearInterval(state.pollTimer);
  state.pollTimer = setInterval(() => {
    if (state.view === "library" && !state.query) refreshLibrary(true);
  }, HOME_REFRESH_MS);
}

// ---- views ----------------------------------------------------------------

function showLibrary() {
  if (state.view === "library") return;
  state.view = "library";
  $("app").dataset.view = "library";
  $("view-settings").hidden = true;
  $("view-library").hidden = false;
  settings.closeSettings();
  updateHomeVisibility();
  $("query").focus();
}

function showSettings() {
  if (state.view === "settings") return;
  closePreview();
  state.view = "settings";
  $("app").dataset.view = "settings";
  $("view-library").hidden = true;
  $("view-settings").hidden = false;
  $("miss").hidden = true;
  settings.openSettings();
}

function resetToHome() {
  state.searchSeq++;
  state.inflightQuery = null;
  clearTimeout(state.searchTimer);
  clearPending();
  $("query").value = "";
  state.query = "";
  state.lens = DEFAULT_LENS;
  paintLenses();
  $("query").placeholder = lensFor(DEFAULT_LENS).placeholder;
  resetResults();
  $("notes").replaceChildren();
  syncClear();
  closePreview();
  showLibrary();
  $("content").scrollTop = 0;
  library.clearSignature();
  library.render();
  updateHomeVisibility();
  refreshLibrary(true);
  $("query").focus();
}

// ---- selection ------------------------------------------------------------

function options() {
  return Array.from($("view-library").querySelectorAll("[data-index]"));
}

function select(index, { focus = false, scroll = true } = {}) {
  if (index < 0 || index >= state.visibleRows.length) return;
  state.selected = index;
  paintSelection();
  const node = $(`result-${index}`);
  if (scroll) node?.scrollIntoView({ block: "nearest" });
  if (focus) node?.focus({ preventScroll: true });
}

function paintSelection() {
  for (const node of options()) {
    const selected = Number(node.dataset.index) === state.selected;
    node.setAttribute("aria-selected", selected ? "true" : "false");
    node.tabIndex = selected ? 0 : -1;
  }
  if (state.selected < 0) {
    const first = options()[0];
    if (first) first.tabIndex = 0;
  }
  const query = $("query");
  if (state.selected >= 0) {
    query.setAttribute("aria-activedescendant", `result-${state.selected}`);
  } else {
    query.removeAttribute("aria-activedescendant");
  }
}

function moveDirection(dx, dy) {
  const rows = state.visibleRows;
  if (!rows.length) return;
  const current = state.selected >= 0 ? $(`result-${state.selected}`) : null;
  const focusFollows = Boolean(current && current === document.activeElement);
  if (!current) {
    select(dy < 0 || dx < 0 ? rows.length - 1 : 0, { focus: false });
    return;
  }
  const from = current.getBoundingClientRect();
  const ox = from.left + from.width / 2;
  const oy = from.top + from.height / 2;
  let best = null;
  let bestScore = Infinity;
  for (const node of options()) {
    if (node === current) continue;
    const rect = node.getBoundingClientRect();
    const vx = rect.left + rect.width / 2 - ox;
    const vy = rect.top + rect.height / 2 - oy;
    // Only nodes with real travel in the pressed direction can be answers.
    const along = vx * dx + vy * dy;
    if (along <= 1) continue;
    const across = Math.abs(vx * dy - vy * dx);
    // A neighbour in the same row or column beats a closer diagonal one.
    const score = along + across * 2;
    if (score < bestScore) {
      bestScore = score;
      best = node;
    }
  }
  if (!best) return;
  select(Number(best.dataset.index), { focus: focusFollows });
}

function moveToEdge(last) {
  if (!state.visibleRows.length) return;
  const focusFollows = Boolean(document.activeElement?.closest?.("[data-index]"));
  select(last ? state.visibleRows.length - 1 : 0, { focus: focusFollows });
}

function openSelected({ reveal = false } = {}) {
  const row = state.visibleRows[state.selected];
  if (!row) return;
  preview.open(row, { reveal });
}

function openPreview(row) {
  if (!row) return;
  preview.open(row);
}

// ---- events ---------------------------------------------------------------

function bindEvents() {
  $("query").addEventListener("input", () => {
    state.query = $("query").value.trim();
    syncClear();
    if (state.view === "settings") showLibrary();
    $("first-run").hidden = true;
    $("miss").hidden = true;
    closePreview();
    scheduleSearch();
  });

  $("search-form").addEventListener("submit", (event) => {
    event.preventDefault();
    openSelected();
  });

  $("clear").addEventListener("click", clearQuery);
  $("settings-key").addEventListener("click", showSettings);
  $("first-run-add").addEventListener("click", chooseFolder);

  $("view-library").addEventListener("click", (event) => {
    const node = event.target.closest?.("[data-index]");
    if (!node) return;
    const index = Number(node.dataset.index);
    select(index, { focus: false });
    openPreview(state.visibleRows[index]);
  });

  $("view-library").addEventListener("focusin", (event) => {
    const node = event.target.closest?.("[data-index]");
    if (!node) return;
    const index = Number(node.dataset.index);
    if (index === state.selected) return;
    select(index, { focus: false, scroll: false });
  });

  $("miss").addEventListener("click", (event) => {
    if (!event.target.closest?.(".miss-lens, .miss-clear")) return;
    closePreview();
  });

  document.addEventListener("keydown", onKeyDown);
  // The wall is justified against its own width, so observe the results
  // column itself. A window-resize event can be dropped while the window is
  // minimized, which would leave every later resize ignored.
  if ("ResizeObserver" in window) {
    new ResizeObserver(([entry]) => onResultsWidth(entry.contentRect.width)).observe(
      $("library-results"),
    );
  }
}

function onKeyDown(event) {
  const mod = event.ctrlKey || event.metaKey;
  const target = event.target;

  if (mod && event.key.toLowerCase() === "k") {
    event.preventDefault();
    showLibrary();
    $("query").focus();
    $("query").select();
    return;
  }
  if (mod && event.key === ",") {
    event.preventDefault();
    showSettings();
    return;
  }
  if (mod && ["1", "2", "3", "4"].includes(event.key)) {
    event.preventDefault();
    setLens(LENSES[Number(event.key) - 1].id);
    return;
  }
  if (event.key === "Escape") {
    handleEscape();
    return;
  }
  if (
    event.key === "/" &&
    !mod &&
    (document.activeElement === document.body || document.activeElement === $("content"))
  ) {
    event.preventDefault();
    $("query").focus();
    return;
  }
  if (state.view !== "library") return;

  const typing = target === $("query") && $("query").value.length > 0;

  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    if (state.visibleRows.length) {
      event.preventDefault();
      moveDirection(0, event.key === "ArrowDown" ? 1 : -1);
    }
    return;
  }
  if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
    if (!typing && state.visibleRows.length) {
      event.preventDefault();
      moveDirection(event.key === "ArrowRight" ? 1 : -1, 0);
    }
    return;
  }
  if ((event.key === "Home" || event.key === "End") && target !== $("query")) {
    if (state.visibleRows.length) {
      event.preventDefault();
      moveToEdge(event.key === "End");
    }
    return;
  }
  if (event.key === "Enter") {
    if (mod && target.closest?.("[data-index]")) {
      event.preventDefault();
      openSelected({ reveal: true });
      return;
    }
    if (target === $("query") || !target.closest?.("button, input, a")) {
      if (state.selected >= 0) {
        event.preventDefault();
        openSelected();
      }
    }
  }
}

function handleEscape() {
  if (preview.isOpen()) {
    preview.close();
    return;
  }
  if (state.view === "settings") {
    showLibrary();
    return;
  }
  if (state.query) {
    clearQuery();
    return;
  }
  // A floating summon leaves with Escape, like any transient panel.
  call("hide_window").catch(() => {});
}

function onResultsWidth(width) {
  const rounded = Math.round(width);
  if (!rounded || rounded === state.resultsWidth) return;
  state.resultsWidth = rounded;
  if (state.view !== "library" || state.showFirstRun) return;
  if (state.query && state.searchPayload) {
    results.render(state.searchPayload);
  } else if (!state.query) {
    library.clearSignature();
    library.render();
  }
  paintSelection();
}

function closePreview() {
  if (preview.isOpen()) preview.close();
}

async function chooseFolder() {
  const button = $("first-run-add");
  button.disabled = true;
  try {
    const path = await call("pick_folder");
    if (!path) return;
    renderNotes($("notes"), [{ text: `reading ${path}. search works while it runs.` }]);
    const summary = await call("index_folder", { path });
    renderNotes($("notes"), [{ text: summary }]);
    toast(summary);
    await refreshLibrary(true);
  } catch (error) {
    renderNotes($("notes"), [
      { text: `Could not read that folder: ${error}`, error: true },
    ]);
    toast(`Could not read that folder: ${error}`, "error");
  } finally {
    button.disabled = false;
  }
}
