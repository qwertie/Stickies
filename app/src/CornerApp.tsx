import { invoke } from '@tauri-apps/api/core';

/**
 * The 6x6 hot-spot in the top-right corner of the primary screen. Same gestures and menu as the
 * tray icon: hover raises the last-used note, click brings every note to the front, double-click
 * creates a note, right-click shows the app menu (built in Rust so the tray can share it).
 */
export function CornerApp() {
  return (
    <div
      className="corner"
      title="Stickies: click to bring all notes to front, double-click for a new note"
      onMouseDown={(e) => e.button === 0 && void invoke('raise_all')}
      onDoubleClick={() => void invoke('create_note')}
      onMouseEnter={() => void invoke('focus_latest')}
      onContextMenu={(e) => {
        e.preventDefault();
        void invoke('show_app_menu');
      }}
    />
  );
}
