import { invoke } from '@tauri-apps/api/core';

/**
 * The 6x6 hot-spot in the top-right corner of the primary screen: hover raises the newest note,
 * click brings every note to the front, double-click creates a note.
 */
export function CornerApp() {
  return (
    <div
      className="corner"
      title="Stickies: click to bring all notes to front, double-click for a new note"
      onMouseDown={() => void invoke('raise_all')}
      onDoubleClick={() => void invoke('create_note')}
      onMouseEnter={() => void invoke('focus_latest')}
    />
  );
}
