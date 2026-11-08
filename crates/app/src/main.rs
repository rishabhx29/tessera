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
//! search tool), and `Ctrl+Alt+T` brings it back. The key deliberately avoids
//! `Ctrl+T`: a system-wide registration of that chord would take "new tab"
//! away from every browser and terminal on the machine.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_global_shortcut::{GlobalShortcutExt as _, ShortcutState};
use tessera_app::engine::{
    parse_lens, AppService, LibraryPayload, Preview, SearchPayload, StatusPayload,
};
use tessera_app::watcher::{WatchHandle, WatchStatus};

/// The summon hotkey, in the one place it is written down. It is also the
/// accelerator printed beside the tray menu's Search item and the first row of
/// the settings shortcut list, so all three surfaces name the same chord.
const HOTKEY: &str = "CTRL+ALT+T";

/// The floating size a summon lands at, in logical pixels. It is deliberately
/// smaller than the configured window: the app the user came from stays
/// visible around the field and its results.
const FLOAT_WIDTH: f64 = 920.0;
const FLOAT_HEIGHT: f64 = 640.0;

/// Breathing room between the floating window and the edge of the work area.
const FLOAT_MARGIN: f64 = 24.0;

/// What the window is allowed to ask for.
struct Shell {
    service: AppService,
    /// `None` until the first folder is indexed, and `Some` while a run owns
    /// the connector. Guarded because the commands arrive on webview threads.
    watch: Mutex<Option<WatchHandle>>,
    /// True between a summon and the moment the window is hidden again. The
    /// late placement pass reads it so it never reopens a dismissed window.
    summon_open: AtomicBool,
    /// Whether the window has ever been shown. The startup fallback uses it to
    /// tell a slow first frame from an app that is already running.
    shown_once: AtomicBool,
}

impl Shell {
    /// Bring the watcher in line with the folders the index now holds.
    ///
    /// Called after an index run and after a rebuild, so a newly added folder
    /// is watched without restarting the app, and a wiped index has a watcher
    /// that re-walks the folders it used to hold.
    fn restart_watch(&self) {
        let roots: Vec<PathBuf> = self
            .service
            .status()
            .map(|status| {
                status
                    .folder_rows
                    .into_iter()
                    .map(|row| PathBuf::from(row.path))
                    .collect()
            })
            .unwrap_or_default();
        let mut slot = self
            .watch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(mut handle) = slot.take() {
            handle.stop();
        }
        if roots.is_empty() {
            return;
        }
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
    mode: Option<String>,
    limit: usize,
) -> Result<SearchPayload, String> {
    let mode = parse_lens(mode.as_deref())?;
    shell.service.search(&query, mode, limit)
}

#[tauri::command]
async fn preview(
    shell: State<'_, Shell>,
    path: String,
    query: String,
    page: Option<u32>,
) -> Result<Preview, String> {
    shell.service.preview(&path, &query, page)
}

/// The home's library: newest indexed files, optionally narrowed to some
/// kinds, with the counts that describe the whole index.
#[tauri::command]
async fn library(
    shell: State<'_, Shell>,
    kinds: Option<Vec<String>>,
    limit: usize,
) -> Result<LibraryPayload, String> {
    shell.service.library(&kinds.unwrap_or_default(), limit)
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

/// A cached list thumbnail for an image the index knows.
///
/// Separate from `preview_image` because the two have different budgets: a
/// preview is one picture at full size, while a result list asks for up to
/// forty at once. The work runs on the blocking pool so a row of 4K
/// screenshots never holds the IPC runtime, and the decode happens after the
/// search has already rendered — it is never on the query path.
#[tauri::command]
async fn thumbnail(shell: State<'_, Shell>, path: String) -> Result<tauri::ipc::Response, String> {
    let service = shell.service.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || service.thumbnail(&path))
        .await
        .map_err(|err| format!("the thumbnail task did not finish: {err}"))??;
    Ok(tauri::ipc::Response::new(bytes))
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
fn confirm_remove_folder(app: AppHandle, path: String) -> bool {
    app.dialog()
        .message(format!(
            "Tessera stops watching {path} and drops its files from the index. \
             Search results from it will be gone. Your files are not touched."
        ))
        .title("Remove this folder?")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Remove".to_string(),
            "Cancel".to_string(),
        ))
        .blocking_show()
}

/// Stop watching one folder and drop its rows from the index. The files on
/// disk are untouched.
#[tauri::command]
async fn remove_folder(shell: State<'_, Shell>, path: String) -> Result<usize, String> {
    // Stop first: a watcher still holding a store handle would write rows
    // back while the folder is being forgotten.
    {
        let mut slot = shell
            .watch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(mut handle) = slot.take() {
            handle.stop();
        }
    }
    let removed = shell.service.remove_root(&path);
    shell.restart_watch();
    removed
}

#[tauri::command]
async fn clear_thumbnails(shell: State<'_, Shell>) -> Result<usize, String> {
    shell.service.clear_thumbnails()
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

/// A summon is a floating palette over the app the user came from, not a
/// second workspace. It leaves a maximized or fullscreen state behind, takes a
/// compact size on the monitor under the pointer, and stays above the window
/// that was in front. The shell also opens the window this way on launch, so
/// the app never appears at a frame the OS remembered from an earlier run.
fn show_window(app: &AppHandle) {
    app.state::<Shell>()
        .summon_open
        .store(true, Ordering::SeqCst);
    app.state::<Shell>()
        .shown_once
        .store(true, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_fullscreen(false);
        let _ = window.unmaximize();
        // While the window is hidden this sets the frame it will appear in. A
        // minimized window ignores it, so the same placement runs again after
        // the restore and once more when the restore has settled.
        place_floating(&window);
        let _ = window.show();
        let _ = window.unminimize();
        place_floating(&window);
        let _ = window.set_always_on_top(true);
        let _ = window.set_focus();

        // Windows finishes a minimize or maximize restore a few frames after
        // `show` returns and can put the old frame back. One late pass puts the
        // compact frame in place again, unless the summon was dismissed first.
        let settle = window.clone();
        let handle = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            if !handle.state::<Shell>().summon_open.load(Ordering::SeqCst) {
                return;
            }
            let _ = settle.show();
            let _ = settle.unminimize();
            let _ = settle.unmaximize();
            place_floating(&settle);
            let _ = settle.set_always_on_top(true);
            let _ = settle.set_focus();
        });
    }
    // Every open starts at home: the window drops the query, results,
    // selection and preview it had, so the tool is ready for a new question.
    let _ = app.emit("tessera-show", ());
}

/// Centre the compact window in the work area of the monitor the pointer is
/// on, so a summon arrives where the user is looking rather than on whichever
/// screen the window happened to occupy last.
fn place_floating(window: &WebviewWindow) {
    let monitor = window
        .cursor_position()
        .ok()
        .and_then(|point| window.monitor_from_point(point.x, point.y).ok().flatten())
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        let _ = window.set_size(tauri::LogicalSize::new(FLOAT_WIDTH, FLOAT_HEIGHT));
        let _ = window.center();
        return;
    };

    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let margin = FLOAT_MARGIN * scale;
    let width = (FLOAT_WIDTH * scale).min((area.size.width as f64 - margin * 2.0).max(320.0));
    let height = (FLOAT_HEIGHT * scale).min((area.size.height as f64 - margin * 2.0).max(240.0));
    let _ = window.set_size(PhysicalSize::new(
        width.round() as u32,
        height.round() as u32,
    ));

    // The size above is the canvas; the frame adds a border. Centre the whole
    // frame in the work area so the visible block sits level.
    let frame = window.outer_size().unwrap_or(PhysicalSize::new(
        width.round() as u32,
        height.round() as u32,
    ));
    let x = area.position.x as f64 + (area.size.width as f64 - frame.width as f64) / 2.0;
    let y = area.position.y as f64 + (area.size.height as f64 - frame.height as f64) / 2.0;
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// The window is created hidden; the page calls this once it has rendered, so
/// the app never appears as a half-painted frame.
#[tauri::command]
fn boot(app: AppHandle) {
    show_window(&app);
}

/// Hide the window without stopping the service, the same state the close
/// button leaves behind. A floating summon leaves with Escape.
#[tauri::command]
fn hide_window(app: AppHandle) {
    app.state::<Shell>()
        .summon_open
        .store(false, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

fn main() {
    let data_dir = tessera_core::cli::data_dir().expect("the OS has no local data directory");
    let models_dir = tessera_core::cli::models_dir(&data_dir, None);
    let shell = Shell {
        service: AppService::new(&data_dir, &models_dir),
        watch: Mutex::new(None),
        summon_open: AtomicBool::new(false),
        shown_once: AtomicBool::new(false),
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
            library,
            preview_image,
            thumbnail,
            status,
            watch_status,
            set_paused,
            index_folder,
            rebuild_index,
            confirm_rebuild,
            remove_folder,
            confirm_remove_folder,
            clear_thumbnails,
            install_models,
            pick_folder,
            autostart_enabled,
            set_autostart,
            reveal,
            boot,
            hide_window
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

            // The window is created hidden and shown only once the page
            // reports that it is ready, so the first frame the user sees is
            // the compact summon and not the rectangle the OS remembered from
            // an earlier run.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                if !handle.state::<Shell>().shown_once.load(Ordering::SeqCst) {
                    show_window(&handle);
                }
            });

            // Reading the roots and verifying the model files is I/O the first
            // frame must not wait for: the window paints, then the watcher
            // starts behind it. The verification is cached in the service, so
            // the window's first status call reuses the answer instead of
            // hashing ~1.3 GB of model files itself.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                handle.state::<Shell>().restart_watch();
            });

            // Old index rows get their picture sizes and line boxes in the
            // background, after the window and the watcher have settled. No
            // progress modal: Settings reads the pending count if the user
            // asks. The thread dies with the process, which is the deadline.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(10));
                let service = handle.state::<Shell>().service.clone();
                match service.backfill_missing_geometry() {
                    Ok(summary) if summary.sized + summary.boxed > 0 => eprintln!(
                        "picture backfill: {} sized, {} prepared, {} skipped, {} failed",
                        summary.sized, summary.boxed, summary.skipped, summary.failed
                    ),
                    Ok(_) => {}
                    Err(err) => eprintln!("the picture backfill did not finish: {err}"),
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // The window is a view of a background service, not the service:
            // closing it must not stop indexing. Quit is in the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                window
                    .app_handle()
                    .state::<Shell>()
                    .summon_open
                    .store(false, Ordering::SeqCst);
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Tessera");
}
