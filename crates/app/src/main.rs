//! The desktop shell: one window, a tray icon, a global hotkey, and a thin
//! command layer over [`tessera_app::AppService`].
//!
//! Every command here is a pass-through. The engine work lives in
//! `engine.rs` and `watcher.rs`, which have no windowing dependency and are
//! tested without a display; keeping the Tauri layer trivial is what makes
//! that split worth having.
//!
//! Two behaviours are shell-only and belong here: closing the window hides it
//! instead of quitting (a search tool that the hotkey cannot reach is not a
//! search tool), and `Ctrl+Alt+Space` brings it back.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_global_shortcut::{GlobalShortcutExt as _, ShortcutState};
use tessera_app::engine::{AppService, Preview, SearchPayload, StatusPayload};
use tessera_app::watcher::{WatchHandle, WatchStatus};

/// The hotkey, in the one place it is written down.
const HOTKEY: &str = "CTRL+ALT+SPACE";

/// What the window is allowed to ask for.
struct Shell {
    service: AppService,
    /// `None` until the first folder is indexed, and `Some` while a run owns
    /// the connector. Guarded because the commands arrive on webview threads.
    watch: Mutex<Option<WatchHandle>>,
}

impl Shell {
    /// Bring the watcher in line with the folders the index now holds.
    ///
    /// Called after an index run and after a rebuild, so a newly added folder
    /// is watched without restarting the app, and a wiped index has a watcher
    /// that re-walks the folders it used to hold.
    fn restart_watch(&self) {
        let folders = self
            .service
            .status()
            .map(|status| status.folders)
            .unwrap_or_default();
        let mut slot = self
            .watch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(mut handle) = slot.take() {
            handle.stop();
        }
        if folders.is_empty() {
            return;
        }
        let roots: Vec<PathBuf> = folders.into_iter().map(PathBuf::from).collect();
        match WatchHandle::start(self.service.data_dir(), self.service.models_dir(), roots) {
            Ok(handle) => *slot = Some(handle),
            // Not fatal: searching still works, and the pane shows "stopped".
            Err(err) => eprintln!("the watcher did not start: {err}"),
        }
    }
}

// -- commands ---------------------------------------------------------------

#[tauri::command]
async fn search(
    shell: State<'_, Shell>,
    query: String,
    limit: usize,
) -> Result<SearchPayload, String> {
    shell.service.search(&query, limit)
}

#[tauri::command]
async fn preview(shell: State<'_, Shell>, path: String, query: String) -> Result<Preview, String> {
    shell.service.preview(&path, &query)
}

/// Image bytes for a path the *index* knows is an image.
///
/// Deliberately not the asset protocol: that would let the webview read any
/// file the app can read. The service refuses anything the index does not hold
/// as an image, so the window's reach is exactly the index's reach.
#[tauri::command]
async fn preview_image(
    shell: State<'_, Shell>,
    path: String,
) -> Result<tauri::ipc::Response, String> {
    shell
        .service
        .image_bytes(&path)
        .map(tauri::ipc::Response::new)
}

#[tauri::command]
async fn status(shell: State<'_, Shell>) -> Result<StatusPayload, String> {
    shell.service.status()
}

#[tauri::command]
fn watch_status(shell: State<'_, Shell>) -> WatchStatus {
    let mut slot = shell
        .watch
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match slot.as_mut() {
        Some(handle) => handle.status(),
        None => WatchStatus {
            running: false,
            paused: false,
            log: Vec::new(),
            summary: None,
        },
    }
}

#[tauri::command]
fn set_paused(shell: State<'_, Shell>, paused: bool) {
    let slot = shell
        .watch
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(handle) = slot.as_ref() {
        handle.set_paused(paused);
    }
}

#[tauri::command]
async fn index_folder(shell: State<'_, Shell>, path: String) -> Result<String, String> {
    let summary = shell.service.index_folder(&path)?;
    shell.restart_watch();
    Ok(summary)
}

#[tauri::command]
async fn rebuild_index(shell: State<'_, Shell>) -> Result<Vec<String>, String> {
    // Stop first: a watcher still holding a store handle would race the
    // deletion, and its rows would describe a file that is no longer there.
    {
        let mut slot = shell
            .watch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(mut handle) = slot.take() {
            handle.stop();
        }
    }
    let folders = shell.service.rebuild_index()?;
    // The folders are re-walked by the watcher, which is exactly the rebuild:
    // the same code path as a first run, so there is no second indexer to
    // drift from the first.
    shell.restart_watch();
    Ok(folders)
}

#[tauri::command]
fn confirm_rebuild(app: AppHandle) -> bool {
    app.dialog()
        .message(
            "This deletes the index and rebuilds it from your folders. Search \
             is unavailable until it finishes. Your files are not touched.",
        )
        .title("Rebuild the index?")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Rebuild".to_string(),
            "Cancel".to_string(),
        ))
        .blocking_show()
}

#[tauri::command]
async fn install_models(shell: State<'_, Shell>, sets: Vec<String>) -> Result<Vec<String>, String> {
    let lines = shell.service.install_models(&sets)?;
    // A newly installed set can change which arms exist, so the watcher is
    // restarted to pick up an embedder it did not have before.
    shell.restart_watch();
    Ok(lines)
}

#[tauri::command]
fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let folder = app.dialog().file().blocking_pick_folder();
    Ok(folder
        .and_then(|path| path.into_path().ok())
        .map(|path| path.display().to_string()))
}

#[tauri::command]
fn autostart_enabled(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    let result = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    result.map_err(|err| format!("{err}"))
}

/// Show the file in the OS file manager, selected.
#[tauri::command]
fn reveal(path: String) -> Result<(), String> {
    let target = PathBuf::from(&path);
    if !target.exists() {
        return Err(format!("{} is no longer on disk", target.display()));
    }
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("explorer");
        command.arg(format!("/select,{}", target.display()));
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg("-R").arg(&target);
        command
    };
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(target.parent().unwrap_or(&target));
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("opening {}: {err}", target.display()))
}

// -- shell ------------------------------------------------------------------

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn main() {
    let data_dir =
        tessera_core::cli::default_data_dir().expect("the OS has no local data directory");
    let models_dir = tessera_core::cli::models_dir(&data_dir, None);
    let shell = Shell {
        service: AppService::new(&data_dir, &models_dir),
        watch: Mutex::new(None),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        show_window(app);
                    }
                })
                .build(),
        )
        .manage(shell)
        .invoke_handler(tauri::generate_handler![
            search,
            preview,
            preview_image,
            status,
            watch_status,
            set_paused,
            index_folder,
            rebuild_index,
            confirm_rebuild,
            install_models,
            pick_folder,
            autostart_enabled,
            set_autostart,
            reveal
        ])
        .setup(|app| {
            let search = MenuItem::with_id(app, "search", "Search", true, Some(HOTKEY))?;
            let quit = MenuItem::with_id(app, "quit", "Quit Tessera", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&search, &quit])?;
            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Tessera — one search bar for everything")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "search" => show_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_window(tray.app_handle());
                    }
                })
                .build(app)?;

            app.global_shortcut()
                .register(HOTKEY)
                .map_err(|err| format!("registering {HOTKEY}: {err}"))?;

            // Reading the roots and verifying the model files is I/O the first
            // frame must not wait for: the window paints, then the watcher
            // appears in the activity pane. The verification is cached in the
            // service, so the window's first status call reuses the answer
            // instead of hashing ~1.3 GB of model files itself.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                handle.state::<Shell>().restart_watch();
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // The window is a view of a background service, not the service:
            // closing it must not stop indexing. Quit is in the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Tessera");
}
