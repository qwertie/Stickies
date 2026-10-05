//! Note windows, the corner hot-spot window and the tray icon. Positions are logical pixels.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Monitor, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::layout::{self, Placement, Rect, Screen};
use crate::AppState;

pub const NOTE_SIZE: u32 = 500;
pub const TOP_MARGIN: i32 = 15;
pub const SLOT_STEP: i32 = 200;
pub const CORNER_LABEL: &str = "corner";
const CORNER_SIZE: f64 = 6.0;
/// On macOS the menu bar owns the screen corner, so the hot-spot is a strip just below it that a
/// mouse flung to the right edge still hits.
const CORNER_HEIGHT: f64 = if cfg!(target_os = "macos") { 60.0 } else { CORNER_SIZE };

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
    let stored = layout::load().windows.get(folder).cloned();
    let home = stored.clone().unwrap_or_else(|| next_placement(app));
    let rect = place(app, &home);
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
    keep_corner_on_top(app);
}

/// Re-asserts the dot's topmost status. Other topmost windows (and our own notes while they are
/// briefly pinned) can end up above it; asking again puts it back at the top of the topmost band.
pub fn keep_corner_on_top(app: &AppHandle) {
    if let Some(corner) = app.get_webview_window(CORNER_LABEL) {
        corner.set_always_on_top(true).ok();
    }
}

/// Raises the note the user focused most recently, or the newest note if none has been focused.
/// `focus` is false for the corner dot's hover gesture: merely sweeping the pointer past the
/// corner should not pull the keyboard away from whatever the user is typing in.
pub fn raise_latest(app: &AppHandle, focus: bool) {
    let state = app.state::<AppState>();
    let last = state.last_focused.lock().unwrap().clone();
    let window = last
        .and_then(|label| app.get_webview_window(&label))
        .or_else(|| state.store.list_notes().last().and_then(|n| app.get_webview_window(&label_for(&n.folder))));
    if let Some(w) = window {
        w.set_always_on_top(true).ok();
        w.set_always_on_top(false).ok();
        if focus {
            w.set_focus().ok();
        }
    }
    keep_corner_on_top(app);
}

pub fn open_corner(app: &AppHandle) -> Result<(), String> {
    let (x, y) = corner_position(app);
    let window = WebviewWindowBuilder::new(app, CORNER_LABEL, WebviewUrl::App("index.html".into()))
        .title("Stickies")
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .min_inner_size(CORNER_SIZE, CORNER_HEIGHT)
        .max_inner_size(CORNER_SIZE, CORNER_HEIGHT)
        .inner_size(CORNER_SIZE, CORNER_HEIGHT)
        .position(x, y)
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
    let w = (CORNER_SIZE * scale).round() as i32;
    let h = (CORNER_HEIGHT * scale).round() as i32;
    if let Ok(hwnd) = window.hwnd() {
        let hwnd = HWND(hwnd.0 as _);
        unsafe {
            SetWindowPos(hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSENDCHANGING).ok();
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
        .inner_size(460.0, 530.0)
        .resizable(false)
        .center()
        .initialization_script("window.__STICKIES__ = { options: true };")
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The one menu shared by the tray icon and the corner dot. The labels double as a cheat sheet
/// for the gestures both of them support.
pub fn build_app_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    Menu::with_items(
        app,
        &[
            &item("new", "New note (double-click)")?,
            &item("show", "Bring all notes to front (click)")?,
            &item("latest", "Show last-used note")?,
            &PredefinedMenuItem::separator(app)?,
            &item("folder", "Open data folder")?,
            &item("options", "Options…")?,
            &PredefinedMenuItem::separator(app)?,
            &item("quit", "Quit Stickies")?,
        ],
    )
}

pub fn handle_app_menu(app: &AppHandle, id: &str) {
    match id {
        "new" => {
            crate::create_and_open(app).ok();
        }
        "show" => raise_all(app),
        "latest" => raise_latest(app, true),
        "folder" => {
            let dir = app.state::<AppState>().store.data_dir.clone();
            tauri_plugin_opener::open_path(dir, None::<&str>).ok();
        }
        "options" => {
            open_options(app).ok();
        }
        "quit" => crate::quit(app),
        _ => {}
    }
}

/// Tray icon with the same menu as the corner dot and its click gestures: click raises all notes,
/// double-click creates one. No hover action: the pointer crosses tray icons constantly.
pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_app_menu(app)?;
    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Stickies")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_app_menu(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } => raise_all(app),
                TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                    crate::create_and_open(app).ok();
                }
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

/// Re-places every note window from its saved "home" placement onto the monitors that exist now
/// (see `place`). After a resolution drop the notes are pulled on screen; when the original
/// arrangement returns they go back where the user left them, because the saved rect is never
/// overwritten by these moves (see `save_layout`).
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
        let home = labels.get(w.label()).and_then(|f| layout.windows.get(f)).cloned();
        let target = place(app, &home.unwrap_or(Placement { rect: current, screen: None }));
        if target.x != current.x || target.y != current.y || target.w != current.w || target.h != current.h {
            w.set_size(LogicalSize::new(target.w, target.h)).ok();
            w.set_position(LogicalPosition::new(target.x, target.y)).ok();
        }
    }
    if let Some(corner) = app.get_webview_window(CORNER_LABEL) {
        let (x, y) = corner_position(app);
        corner.set_position(LogicalPosition::new(x, y)).ok();
    }
}

/// The configured corner of the primary work area. On Windows the work area starts at the true
/// screen top; on macOS it starts below the menu bar, which is exactly where the strip should sit.
fn corner_position(app: &AppHandle) -> (f64, f64) {
    let area = primary_work_area(app);
    let corner = app.state::<AppState>().store.config().dot_corner;
    let (left, bottom) = (corner.ends_with("left"), corner.starts_with("bottom"));
    let x = if left { area.x as f64 } else { (area.x + area.w as i32) as f64 - CORNER_SIZE };
    let top = if cfg!(target_os = "macos") { area.y as f64 } else { 0.0 };
    let y = if bottom { (area.y + area.h as i32) as f64 - CORNER_HEIGHT } else { top };
    (x, y)
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
fn next_placement(app: &AppHandle) -> Placement {
    let screen = primary_screen(app);
    let area = screen.area;
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
    Placement { rect, screen: Some(screen) }
}

/// Where a saved placement goes on the current monitors: proportionally the same spot on the same
/// monitor, or on the primary one if that monitor is gone, so a note flush against the right edge
/// stays there after a resolution change. Sizes stay in logical pixels, shrunk only to fit.
fn place(app: &AppHandle, home: &Placement) -> Rect {
    match &home.screen {
        Some(screen) => {
            let area = monitor_named(app, &screen.monitor).map(|m| logical_work_area(&m));
            rescale(home.rect, &screen.area, &area.unwrap_or_else(|| primary_work_area(app)))
        }
        None => clamp_to_screen(app, home.rect),
    }
}

/// Maps a rect between work areas, keeping its position on each axis as the same fraction of the
/// free space (0 = flush left/top, 1 = flush right/bottom). The result lies within `to`.
fn rescale(rect: Rect, from: &Rect, to: &Rect) -> Rect {
    let (x, w) = rescale_axis(rect.x, rect.w, (from.x, from.w), (to.x, to.w));
    let (y, h) = rescale_axis(rect.y, rect.h, (from.y, from.h), (to.y, to.h));
    Rect { x, y, w, h }
}

fn rescale_axis(pos: i32, size: u32, (from, from_len): (i32, u32), (to, to_len): (i32, u32)) -> (i32, u32) {
    let slack = from_len as f64 - size as f64;
    let fraction = if slack > 0.0 { ((pos - from) as f64 / slack).clamp(0.0, 1.0) } else { 0.0 };
    let size = size.min(to_len);
    (to + (fraction * (to_len - size) as f64).round() as i32, size)
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
    primary_screen(app).area
}

fn primary_screen(app: &AppHandle) -> Screen {
    app.primary_monitor()
        .ok()
        .flatten()
        .map(|m| screen_of(&m))
        .unwrap_or(Screen { monitor: String::new(), area: Rect { x: 0, y: 0, w: 1280, h: 720 } })
}

pub fn screen_of(m: &Monitor) -> Screen {
    Screen { monitor: m.name().cloned().unwrap_or_default(), area: logical_work_area(m) }
}

fn monitor_named(app: &AppHandle, name: &str) -> Option<Monitor> {
    app.available_monitors().unwrap_or_default().into_iter().find(|m| m.name().is_some_and(|n| n == name))
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

#[cfg(test)]
mod tests {
    use super::*;

    const BIG: Rect = Rect { x: 0, y: 0, w: 3840, h: 2100 };
    const SMALL: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 };

    #[test]
    fn rescale_keeps_edges_and_fractions() {
        let flush_right = Rect { x: 3840 - 400, y: 0, w: 400, h: 300 };
        assert_eq!(rescale(flush_right, &BIG, &SMALL), Rect { x: 1920 - 400, y: 0, w: 400, h: 300 });
        let centered = Rect { x: 1720, y: 900, w: 400, h: 300 };
        assert_eq!(rescale(centered, &BIG, &SMALL), Rect { x: 760, y: 370, w: 400, h: 300 });
    }

    #[test]
    fn rescale_is_identity_on_same_area_and_shrinks_to_fit() {
        let r = Rect { x: 1234, y: 567, w: 400, h: 300 };
        assert_eq!(rescale(r, &BIG, &BIG), r);
        let huge = Rect { x: 100, y: 100, w: 3000, h: 1500 };
        assert_eq!(rescale(huge, &BIG, &SMALL), SMALL);
    }
}
