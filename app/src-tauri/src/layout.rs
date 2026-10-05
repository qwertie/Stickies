//! Per-machine window geometry, kept out of the synced data folder so a laptop and a desktop
//! with different screens do not fight over positions. Each rect remembers the monitor and work
//! area it was saved on, so it can be re-mapped proportionally when that screen changes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use crate::store::config_dir;

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Placement {
    #[serde(flatten)]
    pub rect: Rect,
    /// Absent in layouts written by older versions; those rects are only clamped to the screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<Screen>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Screen {
    pub monitor: String,
    pub area: Rect,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub windows: HashMap<String, Placement>,
    pub next_slot: u32,
}

pub fn load() -> Layout {
    fs::read_to_string(config_dir().join("layout.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(layout: &Layout) {
    if let Ok(json) = serde_json::to_string_pretty(layout) {
        fs::create_dir_all(config_dir()).ok();
        fs::write(config_dir().join("layout.json"), json).ok();
    }
}

pub fn update(f: impl FnOnce(&mut Layout)) {
    let mut layout = load();
    f(&mut layout);
    save(&layout);
}
