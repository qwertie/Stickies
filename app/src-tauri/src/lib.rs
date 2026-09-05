mod clipboard;
mod layout;
mod speech;
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
use store::{ArchivedInfo, ImportedFile, Store};

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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteLoad {
    content: String,
    data_dir: PathBuf,
    note_dir: PathBuf,
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
        .manage(AppState {
            store: Store::open(),
            labels: Mutex::new(HashMap::new()),
            last_written: Mutex::new(HashMap::new()),
            programmatic_moves_until: Mutex::new(Instant::now()),
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
            read_clipboard_for_paste,
            copy_to_clipboard,
            resolve_attachment,
            save_layout,
            raise_all,
            focus_latest,
            get_data_dir,
            change_data_dir,
            start_dictation,
            stop_dictation,
            quit_app,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
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

/// Asks every note window to flush its pending save, then exits.
pub fn quit(app: &AppHandle) {
    app.emit("save-now", ()).ok();
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
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
    Ok(NoteLoad { content, data_dir: state.store.data_dir.clone(), note_dir: state.store.note_dir(&folder) })
}

#[tauri::command]
fn write_note(state: State<AppState>, folder: String, content: String) -> Result<(), String> {
    state.last_written.lock().unwrap().insert(folder.clone(), content.clone());
    state.store.write_note(&folder, &content)
}

#[tauri::command]
fn create_note(app: AppHandle) -> Result<String, String> {
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
fn restore_note(app: AppHandle, state: State<AppState>, folder: String) -> Result<String, String> {
    let restored = state.store.restore_note(&folder)?;
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

/// For the context-menu Paste command, which cannot use the browser paste event.
#[tauri::command]
fn read_clipboard_for_paste(state: State<AppState>, folder: String) -> Result<PasteContent, String> {
    let paths = clipboard::read_files();
    let mut files = state.store.import_paths(&folder, &paths)?;
    if files.is_empty() {
        if let Some(bmp) = clipboard::read_bitmap() {
            files.push(state.store.save_attachment(&folder, "pasted-image.bmp", &bmp)?);
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

#[tauri::command]
fn resolve_attachment(state: State<AppState>, folder: String, rel: String) -> PathBuf {
    state.store.resolve(&folder, &rel)
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
fn get_data_dir(state: State<AppState>) -> PathBuf {
    state.store.data_dir.clone()
}

/// Points the app at a new data folder, moving the current notes there if it is empty, then
/// restarts so the watcher and windows pick up the new location.
#[tauri::command]
fn change_data_dir(app: AppHandle, state: State<AppState>, path: PathBuf) -> Result<(), String> {
    if path != state.store.data_dir {
        let target_empty = !path.join("notes").exists();
        if target_empty {
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            for sub in ["notes", "archive"] {
                let src = state.store.data_dir.join(sub);
                if src.exists() {
                    std::fs::rename(&src, path.join(sub)).map_err(|e| e.to_string())?;
                }
            }
        }
        store::save_config(&store::Config { data_dir: Some(path) })?;
        app.restart();
    }
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    quit(&app);
}

#[tauri::command]
fn start_dictation(app: AppHandle, label: String) -> Result<(), String> {
    speech::start(app, label)
}

#[tauri::command]
fn stop_dictation() {
    speech::stop();
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
