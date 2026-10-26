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
  pollTimer: null,
  toastTimer: null,
};

const $ = (id) => document.getElementById(id);

// ---- toast -----------------------------------------------------------------

function toast(message, kind = "info") {
  const el = $("toast");
  el.textContent = message;
  el.dataset.kind = kind;
  el.hidden = false;
  clearTimeout(state.toastTimer);
  state.toastTimer = setTimeout(() => {
    el.hidden = true;
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

async function runSearch() {
  const query = $("query").value.trim();
  state.query = query;
  if (!query) {
    state.results = [];
    state.selected = -1;
    renderResults();
    $("notes").replaceChildren();
    closePreview();
    checkFirstRun();
    return;
  }
  const seq = ++state.searchSeq;
  let payload;
  try {
    payload = await call("search", { query, limit: 40 });
  } catch (error) {
    if (seq !== state.searchSeq) return;
    renderNotes([{ text: String(error), error: true }]);
    state.results = [];
    state.selected = -1;
    renderResults();
    return;
  }
  if (seq !== state.searchSeq) return;
  state.results = payload.rows ?? [];
  state.selected = state.results.length ? 0 : -1;
  renderNotes((payload.notes ?? []).map((text) => ({ text, error: false })));
  renderResults();
  renderSearchMeta(payload);
}

function renderSearchMeta(payload) {
  const hint = $("search-hint");
  if (!payload || !payload.rows?.length) {
    hint.textContent = "Ctrl K";
    return;
  }
  const arms = payload.arms?.length ? ` · ${payload.arms.join(" + ")}` : "";
  hint.textContent = `${payload.rows.length} in ${payload.millis} ms${arms}`;
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

function renderResults() {
  const list = $("results");
  list.replaceChildren();
  $("empty").hidden = !state.query || state.results.length > 0 || !$("first-run").hidden;
  list.setAttribute("aria-activedescendant", "");
  state.results.forEach((row, index) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "result";
    button.id = `result-${index}`;
    button.setAttribute("role", "option");
    button.setAttribute("aria-selected", index === state.selected ? "true" : "false");
    button.dataset.index = String(index);

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

    button.append(name, folder, badges, snippet);
    button.addEventListener("click", () => select(index, true));
    list.append(button);
  });
  if (state.selected >= 0) {
    list.setAttribute("aria-activedescendant", `result-${state.selected}`);
  }
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
    button.setAttribute("aria-selected", button.dataset.index === String(index) ? "true" : "false");
  }
  const row = state.results[index];
  if (open) openPreview(row);
  const active = $(`result-${index}`);
  active?.scrollIntoView({ block: "nearest" });
}

function move(delta) {
  if (!state.results.length) return;
  const next = Math.min(Math.max(state.selected + delta, 0), state.results.length - 1);
  select(next, true);
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
    try {
      const bytes = await call("preview_image", { path: preview.path });
      if (seq !== state.previewSeq) return;
      const url = URL.createObjectURL(new Blob([bytes]));
      state.previewUrl = url;
      const img = document.createElement("img");
      img.alt = preview.name;
      img.src = url;
      body.prepend(img);
    } catch (error) {
      if (seq === state.previewSeq) {
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
  const atBottom = log.scrollTop + log.clientHeight >= log.scrollHeight - 8;
  log.textContent = (watch.log ?? []).slice(-60).join("\n");
  if (atBottom) log.scrollTop = log.scrollHeight;

  $("folders").replaceChildren(
    ...status.folders.map((folder) => {
      const li = document.createElement("li");
      li.textContent = folder;
      return li;
    }),
  );
  if (!status.folders.length) {
    const li = document.createElement("li");
    li.className = "dim";
    li.textContent = "No folders indexed yet.";
    $("folders").append(li);
  }

  const facts = $("index-facts");
  facts.replaceChildren();
  const counts = Object.fromEntries(status.counts ?? []);
  for (const [label, value] of [
    ["Files indexed", counts.indexed ?? 0],
    ["Awaiting OCR", counts.awaiting_ocr ?? 0],
    ["Failed", status.failures.length],
    ["Image vectors", status.image_vectors],
    ["Text vectors", status.text_vectors],
    ["Index size", `${status.index_mb.toFixed(1)} MB`],
  ]) {
    facts.append(term(label), detail(value));
  }

  const failures = $("failures");
  failures.replaceChildren();
  $("failure-count").textContent = String(status.failures.length);
  $("failure-empty").hidden = status.failures.length > 0;
  for (const failure of status.failures.slice(0, 40)) {
    const li = document.createElement("li");
    const path = document.createElement("div");
    path.className = "path";
    path.textContent = failure.path;
    const error = document.createElement("div");
    error.className = "error";
    error.textContent = failure.error;
    li.append(path, error);
    failures.append(li);
  }

  const skips = $("skip-facts");
  skips.replaceChildren();
  $("skip-empty").hidden = status.skip_reasons.length > 0;
  for (const [reason, count] of status.skip_reasons) {
    skips.append(term(reason), detail(count));
  }

  $("activity-dot").hidden = status.failures.length === 0;
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
  $("settings-folders").replaceChildren(
    ...status.folders.map((folder) => {
      const li = document.createElement("li");
      li.textContent = folder;
      return li;
    }),
  );
  if (!status.folders.length) {
    const li = document.createElement("li");
    li.className = "dim";
    li.textContent = "No folders yet. Add one to begin.";
    $("settings-folders").append(li);
  }

  const table = $("models");
  table.replaceChildren();
  const head = document.createElement("tr");
  for (const label of ["Set", "State", ""]) {
    const th = document.createElement("th");
    th.textContent = label;
    head.append(th);
  }
  table.append(head);
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
    table.append(tr);
  }

  const facts = $("data-facts");
  facts.replaceChildren();
  for (const [label, value] of [
    ["Index", status.index_path],
    ["Index size", `${status.index_mb.toFixed(1)} MB`],
    ["Image vectors", status.image_vectors],
    ["Text vectors", status.text_vectors],
  ]) {
    facts.append(term(label), detail(value));
  }

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

$("query").addEventListener("input", () => {
  scheduleSearch();
  closePreview();
});

$("search-form").addEventListener("submit", (event) => {
  event.preventDefault();
  if (state.selected >= 0) select(state.selected, true);
});

$("results").addEventListener("keydown", (event) => {
  if (event.key === "ArrowDown") {
    event.preventDefault();
    move(1);
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    move(-1);
  } else if (event.key === "Enter" && state.selected >= 0) {
    event.preventDefault();
    select(state.selected, true);
  }
});

$("preview-close").addEventListener("click", closePreview);
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
    } else if ($("query").value) {
      $("query").value = "";
      runSearch();
    }
  }
  if ((event.key === "ArrowDown" || event.key === "ArrowUp") && document.activeElement !== $("query")) {
    if (state.view === "search" && state.results.length) {
      event.preventDefault();
      move(event.key === "ArrowDown" ? 1 : -1);
    }
  }
});

// ---- boot ------------------------------------------------------------------

(async function boot() {
  renderResults();
  await checkFirstRun();
  setInterval(checkFirstRun, 6000);
  $("query").focus();
})();
