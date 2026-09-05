//! Note windows, the corner hot-spot window and the tray icon. Positions are logical pixels.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Monitor, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::layout::{self, Rect};
use crate::AppState;

pub const NOTE_SIZE: u32 = 500;
pub const TOP_MARGIN: i32 = 15;
pub const SLOT_STEP: i32 = 200;
pub const CORNER_LABEL: &str = "corner";
const CORNER_SIZE: f64 = 6.0;

pub fn label_for(folder: &str) -> String {
    let mut h = DefaultHasher::new();
    folder.hash(&mut h);
    format!("note-{:x}", h.finish())
}

/// Opens (or focuses) the window for a note folder.
pub fn open_note(app: &AppHandle, folder: &str) -> Result<WebviewWindow, String> {
    let label = label_for(folder);
    if let Some(existing) = app.get_webview_window(&label) {
        existing.set_focus().ok();
        return Ok(existing);
    }
    app.state::<AppState>().labels.lock().unwrap().insert(label.clone(), folder.to_string());
    let stored = layout::load().windows.get(folder).copied();
    let home = stored.unwrap_or_else(|| next_rect(app));
    let rect = clamp_to_screen(app, home);
    let init = format!(
        "window.__STICKIES__ = {{ folder: {}, label: {} }};",
        serde_json::to_string(folder).unwrap(),
        serde_json::to_string(&label).unwrap()
    );
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("Sticky")
        .decorations(false)
        .inner_size(rect.w as f64, rect.h as f64)
        .min_inner_size(160.0, 90.0)
        .position(rect.x as f64, rect.y as f64)
        .initialization_script(&init)
        .build()
        .map_err(|e| e.to_string())?;
    if stored.is_none() {
        layout::update(|l| {
            l.windows.insert(folder.to_string(), home);
        });
    }
    Ok(window)
}

pub fn close_note_window(app: &AppHandle, folder: &str) {
    if let Some(w) = app.get_webview_window(&label_for(folder)) {
        w.close().ok();
    }
    app.state::<AppState>().labels.lock().unwrap().remove(&label_for(folder));
}

pub fn note_windows(app: &AppHandle) -> Vec<WebviewWindow> {
    app.webview_windows().into_iter().filter(|(l, _)| l.starts_with("note-")).map(|(_, w)| w).collect()
}

/// Bumps every note above other applications' windows without leaving them pinned.
pub fn raise_all(app: &AppHandle) {
    for w in note_windows(app) {
        w.set_always_on_top(true).ok();
        w.set_always_on_top(false).ok();
    }
}

/// Raises the note the user focused most recently, or the newest note if none has been focused.
pub fn focus_latest(app: &AppHandle) {
    let state = app.state::<AppState>();
    let last = state.last_focused.lock().unwrap().clone();
    let window = last
        .and_then(|label| app.get_webview_window(&label))
        .or_else(|| state.store.list_notes().last().and_then(|n| app.get_webview_window(&label_for(&n.folder))));
    if let Some(w) = window {
        w.set_always_on_top(true).ok();
        w.set_always_on_top(false).ok();
        w.set_focus().ok();
    }
}

pub fn open_corner(app: &AppHandle) -> Result<(), String> {
    let area = primary_work_area(app);
    let x = (area.x + area.w as i32) as f64 - CORNER_SIZE;
    let window = WebviewWindowBuilder::new(app, CORNER_LABEL, WebviewUrl::App("index.html".into()))
        .title("Stickies")
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .min_inner_size(CORNER_SIZE, CORNER_SIZE)
        .max_inner_size(CORNER_SIZE, CORNER_SIZE)
        .inner_size(CORNER_SIZE, CORNER_SIZE)
        .position(x, 0.0)
        .initialization_script("window.__STICKIES__ = { corner: true };")
        .build()
        .map_err(|e| e.to_string())?;
    force_tiny_size(&window);
    Ok(())
}

/// Windows refuses sizes below its minimum tracking size through the normal path; SetWindowPos
/// with SWP_NOSENDCHANGING skips that check.
#[cfg(windows)]
fn force_tiny_size(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSENDCHANGING, SWP_NOZORDER,
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let px = (CORNER_SIZE * scale).round() as i32;
    if let Ok(hwnd) = window.hwnd() {
        let hwnd = HWND(hwnd.0 as _);
        unsafe {
            SetWindowPos(hwnd, None, 0, 0, px, px, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSENDCHANGING).ok();
        }
    }
}

#[cfg(not(windows))]
fn force_tiny_size(_: &WebviewWindow) {}

pub const OPTIONS_LABEL: &str = "options";

pub fn open_options(app: &AppHandle) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(OPTIONS_LABEL) {
        existing.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(app, OPTIONS_LABEL, WebviewUrl::App("index.html".into()))
        .title("Stickies Options")
        .inner_size(460.0, 470.0)
        .resizable(false)
        .center()
        .initialization_script("window.__STICKIES__ = { options: true };")
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let new_note = MenuItem::with_id(app, "new", "New note", true, None::<&str>)?;
    let show_all = MenuItem::with_id(app, "show", "Bring all notes to front", true, None::<&str>)?;
    let open_dir = MenuItem::with_id(app, "folder", "Open data folder", true, None::<&str>)?;
    let options = MenuItem::with_id(app, "options", "Options…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Stickies", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&new_note, &show_all, &open_dir, &options, &quit])?;
    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Stickies")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "new" => {
                crate::create_and_open(app).ok();
            }
            "show" => raise_all(app),
            "folder" => {
                let dir = app.state::<AppState>().store.data_dir.clone();
                tauri_plugin_opener::open_path(dir, None::<&str>).ok();
            }
            "options" => {
                open_options(app).ok();
            }
            "quit" => crate::quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. } = event {
                raise_all(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Re-places every note window from its saved "home" rect, clamped to the monitors that exist
/// now. After a resolution drop the notes are pulled on screen; when the original arrangement
/// returns they go back where the user left them, because the saved rect is never overwritten by
/// these moves (see `save_layout`).
pub fn clamp_all(app: &AppHandle) {
    let state = app.state::<AppState>();
    *state.programmatic_moves_until.lock().unwrap() = Instant::now() + Duration::from_millis(1500);
    let layout = layout::load();
    let labels = state.labels.lock().unwrap().clone();
    for w in note_windows(app) {
        let scale = w.scale_factor().unwrap_or(1.0);
        let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else { continue };
        let pos = pos.to_logical::<i32>(scale);
        let size = size.to_logical::<u32>(scale);
        let current = Rect { x: pos.x, y: pos.y, w: size.width, h: size.height };
        let home = labels.get(w.label()).and_then(|f| layout.windows.get(f)).copied().unwrap_or(current);
        let target = clamp_to_screen(app, home);
        if target.x != current.x || target.y != current.y || target.w != current.w || target.h != current.h {
            w.set_size(LogicalSize::new(target.w, target.h)).ok();
            w.set_position(LogicalPosition::new(target.x, target.y)).ok();
        }
    }
    if let Some(corner) = app.get_webview_window(CORNER_LABEL) {
        let area = primary_work_area(app);
        corner.set_position(LogicalPosition::new((area.x + area.w as i32) as f64 - CORNER_SIZE, 0.0)).ok();
    }
}

/// A fingerprint of the monitor arrangement; a change means windows may need re-clamping.
pub fn monitor_signature(app: &AppHandle) -> String {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| format!("{:?}{:?}", m.position(), m.size()))
        .collect()
}

/// Left or right edge of the primary work area (per settings), TOP_MARGIN down, stepping
/// SLOT_STEP per new note and wrapping to the top when the next slot would run off the bottom.
fn next_rect(app: &AppHandle) -> Rect {
    let area = primary_work_area(app);
    let on_left = app.state::<AppState>().store.config().new_note_side == "left";
    let mut rect = Rect { x: 0, y: 0, w: NOTE_SIZE, h: NOTE_SIZE };
    layout::update(|l| {
        let mut y = TOP_MARGIN + (l.next_slot as i32) * SLOT_STEP;
        if y + NOTE_SIZE as i32 > area.y + area.h as i32 {
            l.next_slot = 0;
            y = TOP_MARGIN;
        }
        l.next_slot += 1;
        rect.x = if on_left { area.x } else { area.x + area.w as i32 - NOTE_SIZE as i32 };
        rect.y = area.y + y;
    });
    rect
}

/// Clamps into the work area of the monitor containing the rect's top-left, or the primary
/// monitor when that monitor is gone.
fn clamp_to_screen(app: &AppHandle, rect: Rect) -> Rect {
    clamp_rect(rect, &work_area_for_point(app, rect.x + 20, rect.y + 10))
}

fn clamp_rect(mut rect: Rect, area: &Rect) -> Rect {
    rect.w = rect.w.min(area.w);
    rect.h = rect.h.min(area.h);
    rect.x = rect.x.clamp(area.x, area.x + (area.w - rect.w) as i32);
    rect.y = rect.y.clamp(area.y, area.y + (area.h - rect.h) as i32);
    rect
}

fn primary_work_area(app: &AppHandle) -> Rect {
    app.primary_monitor()
        .ok()
        .flatten()
        .map(|m| logical_work_area(&m))
        .unwrap_or(Rect { x: 0, y: 0, w: 1280, h: 720 })
}

fn work_area_for_point(app: &AppHandle, x: i32, y: i32) -> Rect {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(logical_work_area)
        .find(|a| x >= a.x && x < a.x + a.w as i32 && y >= a.y && y < a.y + a.h as i32)
        .unwrap_or_else(|| primary_work_area(app))
}

fn logical_work_area(m: &Monitor) -> Rect {
    let scale = m.scale_factor();
    let area = m.work_area();
    let pos = area.position.to_logical::<i32>(scale);
    let size = area.size.to_logical::<u32>(scale);
    Rect { x: pos.x, y: pos.y, w: size.width, h: size.height }
}
