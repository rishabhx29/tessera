// The quiet settings page: one lowercase column, grouped rows.
//
// Everything here is state the engine already keeps: folders, the watcher, the
// model sets, the index. The page only asks and shows; it never invents a
// setting the app cannot honour.

import { count, el, folderName, plural } from "./dom.js";
import { setTheme, themeChoice } from "./theme.js";

const THEMES = ["system", "light", "dark"];
const DOCUMENT_KINDS = ["pdf", "docx", "text"];
const POLL_MS = 1800;

let ctx = null;
let root = null;
let nodes = null;
let pollTimer = null;
let inflight = false;

export function initSettings(rootNode, context) {
  ctx = context;
  root = rootNode;
  build();
  bind();
}

export function openSettings() {
  if (!nodes) return;
  refresh(false);
  startPolling();
  nodes.back.focus({ preventScroll: true });
}

export function closeSettings() {
  stopPolling();
}

// ---- markup ---------------------------------------------------------------

function build() {
  const page = el("div", "settings-page");

  const head = el("header", "settings-head");
  const title = el("h1", "", "settings");
  const back = el("button", "settings-back");
  back.type = "button";
  back.id = "settings-back";
  back.setAttribute("aria-label", "back to the library");
  back.append(el("kbd", "", "esc"), document.createTextNode(" back"));
  back.addEventListener("click", () => ctx.showLibrary());
  head.append(title, back);
  page.append(head);

  const folders = section("folders");
  const folderBox = el("div", "sec-box");
  const folderList = el("div");
  folderList.id = "folder-list";
  const addRow = el("div", "sec-row");
  const addButton = el("button", "primary", "add folder");
  addButton.type = "button";
  addButton.id = "add-folder";
  addButton.addEventListener("click", addFolder);
  addRow.append(
    rowMain("add a folder", "Tessera reads it once, then watches it for changes."),
    actions(addButton),
  );
  const indexing = el("p", "sec-note");
  indexing.id = "indexing-note";
  indexing.hidden = true;
  indexing.setAttribute("role", "status");
  folderBox.append(folderList, addRow, indexing);
  folders.box.append(folderBox);
  page.append(folders.container);

  const look = section("look");
  const themeRow = el("div", "sec-row");
  const themeSeg = el("div", "seg");
  themeSeg.setAttribute("role", "group");
  themeSeg.setAttribute("aria-label", "theme");
  for (const value of THEMES) {
    const button = el("button", "", value);
    button.type = "button";
    button.dataset.theme = value;
    button.setAttribute("aria-pressed", "false");
    button.addEventListener("click", () => {
      setTheme(value);
      paintTheme();
    });
    themeSeg.append(button);
  }
  themeRow.append(rowMain("theme", "follow the system, or choose one."), themeSeg);
  look.box.append(themeRow);
  page.append(look.container);

  const reading = section("reading");
  const watchRow = el("div", "sec-row");
  const watchValue = el("span", "sec-value dim", "");
  const watchButton = el("button", "quiet", "pause");
  watchButton.type = "button";
  watchButton.addEventListener("click", toggleWatch);
  watchRow.append(
    rowMain("watching folders", "new files appear without a rebuild."),
    watchValue,
    actions(watchButton),
  );

  const ocrRow = el("div", "sec-row");
  const ocrValue = el("span", "sec-value", "");
  ocrRow.append(
    rowMain("text recognition", "the on-device reader for words inside pictures."),
    ocrValue,
  );

  const highlightRow = el("div", "sec-row");
  const highlightValue = el("span", "sec-value", "");
  highlightRow.append(
    rowMain("reading older pictures", "boxes on words appear as these are read."),
    highlightValue,
  );

  const startRow = el("div", "sec-row");
  const autostart = document.createElement("input");
  autostart.type = "checkbox";
  autostart.id = "autostart";
  autostart.addEventListener("change", toggleAutostart);
  startRow.append(
    rowMain("start when i sign in", "the watcher runs at idle priority and pauses on battery."),
    actions(autostart),
  );

  reading.box.append(watchRow, ocrRow, highlightRow, startRow);
  page.append(reading.container);

  const models = section("models");
  const table = el("table", "models");
  const note = el("p", "sec-note");
  note.setAttribute("role", "status");
  models.box.append(table, note);
  page.append(models.container);

  const storage = section("storage");
  const facts = el("dl", "facts");
  const factNodes = {
    index: fact(facts, "index"),
    size: fact(facts, "index size"),
    pictures: fact(facts, "pictures"),
    documents: fact(facts, "documents"),
  };
  const clearRow = el("div", "sec-row");
  const clearButton = el("button", "quiet", "clear");
  clearButton.type = "button";
  clearButton.addEventListener("click", () => clearThumbnails(clearButton));
  clearRow.append(
    rowMain(
      "thumbnail cache",
      "removes cached pictures only. the index and your files stay.",
    ),
    actions(clearButton),
  );
  const rebuildRow = el("div", "sec-row");
  const rebuildButton = el("button", "danger", "rebuild…");
  rebuildButton.type = "button";
  rebuildButton.addEventListener("click", () => rebuildIndex(rebuildButton));
  rebuildRow.append(
    rowMain(
      "rebuild the index",
      "reads every folder again. searches are empty until it finishes.",
    ),
    actions(rebuildButton),
  );
  storage.box.append(facts, clearRow, rebuildRow);
  page.append(storage.container);

  const shortcuts = section("shortcuts");
  shortcuts.box.append(shortcutKeys());
  page.append(shortcuts.container);

  const about = section("about");
  const aboutRow = el("div", "sec-row");
  const version = el("span", "sec-value", "tessera");
  aboutRow.append(
    rowMain("tessera", "your files never leave this machine. MIT licence."),
    version,
  );
  about.box.append(aboutRow);
  page.append(about.container);

  const foot = el("p", "settings-foot");
  foot.append(el("kbd", "", "esc"), document.createTextNode(" back"));
  page.append(foot);

  root.replaceChildren(page);

  nodes = {
    back,
    folderList,
    addButton,
    indexing,
    theme: themeSeg,
    watchValue,
    watchButton,
    ocrValue,
    highlightRow,
    highlightValue,
    autostart,
    models: table,
    modelsNote: note,
    facts: factNodes,
    clearButton,
    rebuildButton,
    about: version,
  };
  paintTheme();
}

function section(label) {
  const container = el("section", "sec");
  const box = el("div", "sec-box");
  container.append(el("p", "sec-label", label), box);
  return { container, box };
}

function rowMain(title, hint) {
  const main = el("div", "sec-main");
  main.append(el("p", "sec-title", title));
  if (hint) main.append(el("p", "sec-hint", hint));
  return main;
}

function actions(...children) {
  const box = el("div", "sec-actions");
  box.append(...children);
  return box;
}

function fact(parent, label) {
  const box = el("div", "fact");
  box.append(el("dt", "", label), el("dd", "", ""));
  parent.append(box);
  return box.querySelector("dd");
}

function shortcutKeys() {
  const list = el("dl", "keys");
  const rows = [
    ["ctrl k", "focus the field"],
    ["ctrl ,", "settings"],
    ["esc", "close, clear or go back"],
    ["ctrl 1..4", "all, pictures, words, documents"],
    ["/", "focus the field"],
    ["up, down", "move the selection"],
    ["left, right", "move across the wall"],
    ["home, end", "first or last result"],
    ["enter", "open the selected file"],
    ["ctrl enter", "open and show in folder"],
    ["tab", "field, lenses, wall, preview"],
  ];
  for (const [key, what] of rows) {
    list.append(el("dt", "", key), el("dd", "", what));
  }
  return list;
}

function bind() {
  ctx.call("autostart_enabled")
    .then((enabled) => {
      nodes.autostart.checked = Boolean(enabled);
    })
    .catch(() => {
      nodes.autostart.checked = false;
    });
}

function paintTheme() {
  const choice = themeChoice();
  for (const button of nodes.theme.querySelectorAll("button")) {
    button.setAttribute("aria-pressed", button.dataset.theme === choice ? "true" : "false");
  }
}

// ---- data -----------------------------------------------------------------

async function refresh(quiet) {
  if (inflight) return;
  inflight = true;
  let status;
  let watch = null;
  try {
    status = await ctx.call("status");
  } catch (error) {
    inflight = false;
    if (!quiet) ctx.toast(`Could not read settings: ${error}`, "error");
    return;
  }
  try {
    watch = await ctx.call("watch_status");
  } catch {
    watch = null;
  }
  inflight = false;
  renderFolders(status);
  renderReading(status, watch);
  renderModels(status);
  renderStorage(status);
  renderAbout(status);
}

function renderFolders(status) {
  const rows = status.folder_rows ?? [];
  const signature = JSON.stringify(rows.map((row) => [row.path, row.count]));
  if (nodes.folderList.dataset.signature === signature) return;
  nodes.folderList.dataset.signature = signature;
  nodes.folderList.replaceChildren();
  if (!rows.length) {
    nodes.folderList.append(
      el("p", "sec-empty", "no folders yet. add one to begin."),
    );
    return;
  }
  for (const row of rows) {
    const line = el("div", "folder-row");
    const main = el("div", "folder-main");
    main.append(
      el("span", "folder-name", folderName(row.path)),
      el("span", "folder-path", row.path),
    );
    const files = `${count(row.count)} ${plural(row.count, "file", "files")}`;
    const remove = el("button", "quiet", "remove");
    remove.type = "button";
    remove.setAttribute("aria-label", `remove ${row.path} from the index`);
    remove.addEventListener("click", () => removeFolder(row.path, remove));
    line.append(main, el("span", "folder-count", files), remove);
    nodes.folderList.append(line);
  }
}

function renderReading(status, watch) {
  const running = Boolean(watch?.running);
  const paused = Boolean(watch?.paused);
  nodes.watchValue.textContent = !running ? "not watching" : paused ? "paused" : "watching";
  nodes.watchValue.classList.toggle("dim", !running);
  nodes.watchButton.hidden = !running;
  nodes.watchButton.textContent = paused ? "resume" : "pause";
  nodes.watchButton.dataset.paused = paused ? "true" : "false";

  nodes.ocrValue.textContent = status.ocr_engine || "not reported";

  const pending = Number(status.highlight_pending) || 0;
  nodes.highlightRow.hidden = pending <= 0;
  nodes.highlightValue.textContent = `${count(pending)} left`;
}

function renderModels(status) {
  const sets = status.models ?? [];
  const signature = JSON.stringify(sets);
  if (nodes.models.dataset.signature === signature) return;
  nodes.models.dataset.signature = signature;
  nodes.models.replaceChildren();
  const head = document.createElement("tr");
  for (const label of ["set", "state", ""]) {
    const cell = document.createElement("th");
    cell.textContent = label;
    head.append(cell);
  }
  nodes.models.append(head);
  for (const set of sets) {
    const line = document.createElement("tr");
    const name = document.createElement("td");
    name.textContent = set.name;
    const state = document.createElement("td");
    state.className = set.complete ? "state" : "state missing";
    state.textContent = set.complete
      ? "installed"
      : `${count(set.missing?.length ?? 0)} ${plural(set.missing?.length ?? 0, "file", "files")} missing`;
    const action = document.createElement("td");
    if (!set.complete) {
      const button = el("button", "ghost", "download");
      button.type = "button";
      button.addEventListener("click", () => installModels(set.name, button));
      action.append(button);
    }
    line.append(name, state, action);
    nodes.models.append(line);
  }
}

function renderStorage(status) {
  const byKind = new Map(status.counts_by_kind ?? []);
  const pictures = byKind.get("image") ?? 0;
  const documents = DOCUMENT_KINDS.reduce((total, kind) => total + (byKind.get(kind) ?? 0), 0);
  nodes.facts.index.textContent = status.index_path || "";
  nodes.facts.index.title = status.index_path || "";
  nodes.facts.size.textContent = `${(Number(status.index_mb) || 0).toFixed(1)} MB`;
  nodes.facts.pictures.textContent = count(pictures);
  nodes.facts.documents.textContent = count(documents);
}

function renderAbout(status) {
  nodes.about.textContent = `tessera ${status.version || "0.1.0"}`;
}

// ---- actions --------------------------------------------------------------

async function addFolder() {
  let path;
  try {
    path = await ctx.call("pick_folder");
  } catch (error) {
    ctx.toast(`Could not open the folder picker: ${error}`, "error");
    return;
  }
  if (!path) return;
  nodes.indexing.hidden = false;
  nodes.indexing.textContent = `reading ${path}. search works while it runs.`;
  nodes.addButton.disabled = true;
  try {
    const summary = await ctx.call("index_folder", { path });
    nodes.indexing.textContent = summary;
    ctx.toast(summary);
    await refresh(true);
    await ctx.refreshLibrary(true);
  } catch (error) {
    nodes.indexing.textContent = `could not read that folder: ${error}`;
    ctx.toast(`Could not read that folder: ${error}`, "error");
  } finally {
    nodes.addButton.disabled = false;
  }
}

async function removeFolder(path, button) {
  button.disabled = true;
  try {
    const confirmed = await ctx.call("confirm_remove_folder", { path });
    if (!confirmed) return;
    await ctx.call("remove_folder", { path });
    ctx.toast(`Removed ${folderName(path)} from the index. Your files are not touched.`);
    await refresh(true);
    await ctx.refreshLibrary(true);
  } catch (error) {
    ctx.toast(`Could not remove that folder: ${error}`, "error");
  } finally {
    button.disabled = false;
  }
}

async function toggleWatch() {
  const paused = nodes.watchButton.dataset.paused === "true";
  try {
    await ctx.call("set_paused", { paused: !paused });
    await refresh(true);
  } catch (error) {
    ctx.toast(`Could not change the watcher: ${error}`, "error");
  }
}

async function toggleAutostart() {
  const enabled = nodes.autostart.checked;
  try {
    await ctx.call("set_autostart", { enabled });
    ctx.toast(enabled ? "Tessera will start when you sign in." : "Autostart off.");
  } catch (error) {
    nodes.autostart.checked = !enabled;
    ctx.toast(`Could not change autostart: ${error}`, "error");
  }
}

async function installModels(name, button) {
  const label = button.textContent;
  button.disabled = true;
  button.textContent = "downloading…";
  nodes.modelsNote.textContent =
    "downloading and verifying. this is the only network access the app makes, and only because you asked.";
  try {
    const lines = await ctx.call("install_models", { sets: [name] });
    nodes.modelsNote.textContent = lines.join(" · ") || "models installed.";
  } catch (error) {
    nodes.modelsNote.textContent = `download failed: ${error}`;
  } finally {
    button.disabled = false;
    button.textContent = label;
    await refresh(true);
  }
}

async function clearThumbnails(button) {
  button.disabled = true;
  try {
    const removed = await ctx.call("clear_thumbnails");
    ctx.toast(
      `Cleared ${count(removed)} cached ${plural(removed, "picture", "pictures")}.`,
    );
    await refresh(true);
  } catch (error) {
    ctx.toast(`Could not clear the cache: ${error}`, "error");
  } finally {
    button.disabled = false;
  }
}

async function rebuildIndex(button) {
  button.disabled = true;
  try {
    const confirmed = await ctx.call("confirm_rebuild");
    if (!confirmed) return;
    const folders = await ctx.call("rebuild_index");
    ctx.toast(
      folders.length
        ? `Index deleted. Rebuilding ${count(folders.length)} ${plural(folders.length, "folder", "folders")}.`
        : "Index deleted. Add a folder to begin.",
    );
    await refresh(true);
    await ctx.refreshLibrary(true);
  } catch (error) {
    ctx.toast(`Could not rebuild the index: ${error}`, "error");
  } finally {
    button.disabled = false;
  }
}

// ---- polling --------------------------------------------------------------

function startPolling() {
  stopPolling();
  pollTimer = setInterval(() => refresh(true), POLL_MS);
}

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}
