mod clipboard;
mod layout;
mod store;
mod watcher;
mod windows;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use layout::Rect;
use store::{ArchivedInfo, Config, ImportedFile, Store};

pub struct AppState {
    pub store: Store,
    /// Window label -> note folder.
    pub labels: Mutex<HashMap<String, String>>,
    /// Note folder -> last content this process wrote or loaded, so the watcher can ignore our
    /// own writes.
    pub last_written: Mutex<HashMap<String, String>>,
    /// Until this instant, window move/resize events are the app's own doing (clamping after a
    /// monitor change) and must not be saved as the user's chosen layout.
    pub programmatic_moves_until: Mutex<Instant>,
    /// Label of the note window the user focused most recently; the corner dot raises this one.
    pub last_focused: Mutex<Option<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteLoad {
    content: String,
    data_dir: PathBuf,
    note_dir: PathBuf,
    defaults: Config,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteContent {
    text: Option<String>,
    files: Vec<ImportedFile>,
}

pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| windows::raise_all(app)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .on_menu_event(|app, event| windows::handle_app_menu(app, event.id.as_ref()))
        .manage(AppState {
            store: Store::open(),
            labels: Mutex::new(HashMap::new()),
            last_written: Mutex::new(HashMap::new()),
            programmatic_moves_until: Mutex::new(Instant::now()),
            last_focused: Mutex::new(None),
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(true) = event {
                if window.label().starts_with("note-") {
                    *window.state::<AppState>().last_focused.lock().unwrap() = Some(window.label().to_string());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_note,
            write_note,
            create_note,
            close_note,
            list_archived,
            restore_note,
            save_attachment,
            import_clipboard_files,
            import_files,
            read_clipboard_for_paste,
            copy_to_clipboard,
            open_in_explorer,
            save_layout,
            raise_all,
            focus_latest,
            get_settings,
            save_settings,
            open_options,
            show_app_menu,
            quit_app,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // menu bar extra, no Dock icon
            let state = app.state::<AppState>();
            state.store.purge_archive();
            app.asset_protocol_scope().allow_directory(&state.store.data_dir, true)?;
            app.autolaunch().enable().ok();
            windows::build_tray(&handle)?;
            windows::open_corner(&handle)?;
            for note in state.store.list_notes() {
                windows::open_note(&handle, &note.folder).ok();
            }
            let debouncer = watcher::start(handle.clone())?;
            app.manage(Mutex::new(debouncer));
            spawn_monitor_poll(handle.clone());
            spawn_daily_purge(handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Stickies")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { api, code: None, .. } = event {
                api.prevent_exit();
            }
        });
}

/// Asks every note window to flush its pending save, deletes attachment files no open note
/// references any more (see `Store::prune_attachments` for why not earlier), then exits.
pub fn quit(app: &AppHandle) {
    app.emit("save-now", ()).ok();
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        let state = app.state::<AppState>();
        let open: Vec<String> = state.labels.lock().unwrap().values().cloned().collect();
        for folder in open {
            state.store.prune_attachments(&folder);
        }
        app.exit(0);
    });
}

pub fn create_and_open(app: &AppHandle) -> Result<String, String> {
    let folder = app.state::<AppState>().store.create_note()?;
    windows::open_note(app, &folder)?;
    Ok(folder)
}

#[tauri::command]
fn load_note(state: State<AppState>, folder: String) -> Result<NoteLoad, String> {
    let content = state.store.read_note(&folder)?;
    state.last_written.lock().unwrap().insert(folder.clone(), content.clone());
    Ok(NoteLoad {
        content,
        data_dir: state.store.data_dir.clone(),
        note_dir: state.store.note_dir(&folder),
        defaults: state.store.config(),
    })
}

#[tauri::command]
fn write_note(state: State<AppState>, folder: String, content: String) -> Result<(), String> {
    state.last_written.lock().unwrap().insert(folder.clone(), content.clone());
    state.store.write_note(&folder, &content)
}

/// Async on purpose: on Windows, creating a window from a synchronous command deadlocks the IPC
/// thread against the main thread (documented Tauri gotcha). Same for `restore_note`.
#[tauri::command]
async fn create_note(app: AppHandle) -> Result<String, String> {
    create_and_open(&app)
}

/// Archives the note and closes its window.
#[tauri::command]
fn close_note(app: AppHandle, state: State<AppState>, folder: String) -> Result<(), String> {
    windows::close_note_window(&app, &folder);
    layout::update(|l| {
        l.windows.remove(&folder);
    });
    state.last_written.lock().unwrap().remove(&folder);
    state.store.archive_note(&folder)
}

#[tauri::command]
fn list_archived(state: State<AppState>) -> Vec<ArchivedInfo> {
    state.store.list_archived()
}

#[tauri::command]
async fn restore_note(app: AppHandle, folder: String) -> Result<String, String> {
    let restored = app.state::<AppState>().store.restore_note(&folder)?;
    windows::open_note(&app, &restored)?;
    Ok(restored)
}

#[tauri::command]
fn save_attachment(state: State<AppState>, folder: String, name: String, bytes: Vec<u8>) -> Result<ImportedFile, String> {
    state.store.save_attachment(&folder, &name, &bytes)
}

/// Copies whatever files/folders are on the clipboard (CF_HDROP) into the note's attachments.
#[tauri::command]
fn import_clipboard_files(state: State<AppState>, folder: String) -> Result<Vec<ImportedFile>, String> {
    let paths = clipboard::read_files();
    state.store.import_paths(&folder, &paths)
}

/// Copies files dropped from Explorer into the note's attachments.
#[tauri::command]
fn import_files(state: State<AppState>, folder: String, paths: Vec<PathBuf>) -> Result<Vec<ImportedFile>, String> {
    state.store.import_paths(&folder, &paths)
}

/// For the context-menu Paste command, which cannot use the browser paste event.
#[tauri::command]
fn read_clipboard_for_paste(state: State<AppState>, folder: String) -> Result<PasteContent, String> {
    let paths = clipboard::read_files();
    let mut files = state.store.import_paths(&folder, &paths)?;
    if files.is_empty() {
        if let Some((bytes, ext)) = clipboard::read_image() {
            files.push(state.store.save_attachment(&folder, &format!("pasted-image.{ext}"), &bytes)?);
        }
    }
    let text = if files.is_empty() { clipboard::read_text() } else { None };
    Ok(PasteContent { text, files })
}

#[tauri::command]
fn copy_to_clipboard(
    state: State<AppState>,
    folder: String,
    text: String,
    html: Option<String>,
    attachments: Vec<String>,
) -> Result<(), String> {
    let paths: Vec<PathBuf> = attachments.iter().map(|rel| state.store.resolve(&folder, rel)).collect();
    clipboard::write(&text, html.as_deref(), &paths)
}

/// Opens a note folder, or an attachment inside it, with the shell (Explorer or the file's default
/// program). Done in Rust because the opener plugin's JS side needs a static path allow-list.
#[tauri::command]
fn open_in_explorer(state: State<AppState>, folder: String, rel: Option<String>) -> Result<(), String> {
    let path = match rel {
        Some(rel) => state.store.resolve(&folder, &rel),
        None => state.store.note_dir(&folder),
    };
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Config {
    state.store.config()
}

/// Saves settings. A changed data folder moves the notes there (if the target is empty) and
/// restarts the app so the watcher and windows follow.
#[tauri::command]
fn save_settings(app: AppHandle, state: State<AppState>, mut settings: Config) -> Result<(), String> {
    settings.archive_days = settings.archive_days.max(1);
    let new_dir = settings.data_dir.clone().unwrap_or_else(store::default_data_dir);
    let moving = new_dir != state.store.data_dir;
    if moving && !new_dir.join("notes").exists() {
        std::fs::create_dir_all(&new_dir).map_err(|e| e.to_string())?;
        for sub in ["notes", "archive"] {
            let src = state.store.data_dir.join(sub);
            if src.exists() {
                std::fs::rename(&src, new_dir.join(sub)).map_err(|e| e.to_string())?;
            }
        }
    }
    store::save_config(&settings)?;
    *state.store.config.lock().unwrap() = settings;
    if moving {
        app.restart();
    }
    Ok(())
}

#[tauri::command]
async fn open_options(app: AppHandle) -> Result<(), String> {
    windows::open_options(&app)
}

/// Right-click on the corner dot: pops up the same menu the tray icon uses.
#[tauri::command]
async fn show_app_menu(app: AppHandle, window: tauri::Window) -> Result<(), String> {
    use tauri::menu::ContextMenu;
    let menu = windows::build_app_menu(&app).map_err(|e| e.to_string())?;
    menu.popup(window).map_err(|e| e.to_string())
}

/// Records where the user put a window. Moves the app made itself (clamping) are ignored so the
/// saved rect stays the user's intended "home" position.
#[tauri::command]
fn save_layout(state: State<AppState>, folder: String, x: i32, y: i32, w: u32, h: u32) {
    if Instant::now() >= *state.programmatic_moves_until.lock().unwrap() {
        layout::update(|l| {
            l.windows.insert(folder, Rect { x, y, w, h });
        });
    }
}

#[tauri::command]
fn raise_all(app: AppHandle) {
    windows::raise_all(&app);
}

#[tauri::command]
fn focus_latest(app: AppHandle) {
    windows::focus_latest(&app);
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    quit(&app);
}

fn spawn_monitor_poll(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last = windows::monitor_signature(&app);
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let now = windows::monitor_signature(&app);
            if now != last {
                last = now;
                windows::clamp_all(&app);
            }
        }
    });
}

fn spawn_daily_purge(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(24 * 3600));
        app.state::<AppState>().store.purge_archive();
    });
}
