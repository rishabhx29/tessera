// The look: system, light or dark.
//
// One localStorage key, one data attribute on <html>, one media query. The
// choice persists; "system" follows the OS and changes when the OS changes.

const KEY = "tessera.theme";
const CHOICES = ["system", "light", "dark"];

let choice = "system";
let media = null;

function resolve(value) {
  if (value === "light" || value === "dark") return value;
  return globalThis.matchMedia?.("(prefers-color-scheme: dark)")?.matches ? "dark" : "light";
}

function paint() {
  document.documentElement.dataset.theme = resolve(choice);
}

export function themeChoice() {
  return choice;
}

export function currentTheme() {
  return resolve(choice);
}

export function applyTheme(value) {
  choice = CHOICES.includes(value) ? value : "system";
  paint();
}

export function setTheme(value) {
  try {
    localStorage.setItem(KEY, value);
  } catch {
    // A window that cannot remember the choice still honours it for this run.
  }
  applyTheme(value);
}

export function initTheme() {
  let saved = null;
  try {
    saved = localStorage.getItem(KEY);
  } catch {
    saved = null;
  }
  applyTheme(saved);
  media = globalThis.matchMedia?.("(prefers-color-scheme: dark)") ?? null;
  media?.addEventListener?.("change", () => {
    if (choice === "system") paint();
  });
  return currentTheme();
}
