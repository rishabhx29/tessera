// The one door to the Rust side. Everything else in the window is DOM.
//
// The dev mock installs `window.__TAURI__` before this module loads, so the
// same code path runs in a plain browser and in the packaged app.

const tauri = globalThis.window?.__TAURI__;

export const isTauri = Boolean(tauri?.core?.invoke);

export function call(command, args = {}) {
  if (!isTauri) {
    return Promise.reject(new Error(`the ${command} command is unavailable here`));
  }
  return tauri.core.invoke(command, args);
}

export function listen(event, handler) {
  if (!tauri?.event?.listen) return Promise.resolve(() => {});
  return tauri.event.listen(event, handler);
}
