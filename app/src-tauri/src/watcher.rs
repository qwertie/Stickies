//! Watches `<data>/notes` so that edits made by another program (OneDrive, Obsidian, the morning
//! report generator) show up in open windows, new folders open as windows, and deleted folders
//! close theirs.

use std::path::Path;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{windows, AppState};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteChanged {
    pub folder: String,
    pub content: String,
}

pub fn start(app: AppHandle) -> notify::Result<Debouncer<notify::RecommendedWatcher>> {
    let notes_dir = app.state::<AppState>().store.notes_dir();
    let watched = notes_dir.clone();
    let mut debouncer = new_debouncer(Duration::from_millis(400), move |res: DebounceEventResult| {
        if let Ok(events) = res {
            for e in &events {
                log::debug!("fs event {:?} {:?}", e.kind, e.path);
            }
            let mut folders: Vec<String> = events
                .iter()
                .filter_map(|e| folder_of(&watched, &e.path))
                .collect();
            folders.sort();
            folders.dedup();
            for folder in folders {
                handle_folder(&app, &folder);
            }
        }
    })?;
    debouncer.watcher().watch(&notes_dir, RecursiveMode::Recursive)?;
    Ok(debouncer)
}

fn handle_folder(app: &AppHandle, folder: &str) {
    log::info!("note folder changed on disk: {folder}");
    let state = app.state::<AppState>();
    if !state.store.note_dir(folder).is_dir() {
        windows::close_note_window(app, folder);
        state.last_written.lock().unwrap().remove(folder);
        return;
    }
    let Ok(content) = state.store.read_note(folder) else { return };
    let unchanged = state.last_written.lock().unwrap().get(folder) == Some(&content);
    if unchanged {
        return;
    }
    state.last_written.lock().unwrap().insert(folder.to_string(), content.clone());
    if app.get_webview_window(&windows::label_for(folder)).is_some() {
        app.emit("note-changed", NoteChanged { folder: folder.to_string(), content }).ok();
    } else {
        windows::open_note(app, folder).ok();
    }
}

/// First path component below `notes_dir`. Compares case-insensitively and ignores the `\\?\`
/// verbatim prefix, since the watcher may report paths in a different form than we passed in.
fn folder_of(notes_dir: &Path, path: &Path) -> Option<String> {
    let base = normalize(notes_dir);
    let full = normalize(path);
    let rel = full.strip_prefix(&base).ok()?;
    let first = rel.components().next()?;
    let name = first.as_os_str().to_string_lossy().to_string();
    if name.is_empty() || name.ends_with(".tmp") {
        None
    } else {
        // Return the folder name with its on-disk casing.
        let original: Vec<_> = path.components().collect();
        let depth = base.components().count();
        Some(original.get(depth).map(|c| c.as_os_str().to_string_lossy().to_string()).unwrap_or(name))
    }
}

fn normalize(path: &Path) -> std::path::PathBuf {
    let s = path.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    std::path::PathBuf::from(s.to_lowercase())
}
