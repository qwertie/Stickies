import { invoke } from '@tauri-apps/api/core';

/** The 6x6 hot-spot in the top-right corner of the primary screen. */
export function CornerApp() {
  return (
    <div
      className="corner"
      title="Stickies: click to bring all notes to front"
      onMouseDown={() => void invoke('raise_all')}
      onMouseEnter={() => void invoke('focus_latest')}
    />
  );
}
