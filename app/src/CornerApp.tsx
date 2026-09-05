import { invoke } from '@tauri-apps/api/core';
import { Menu, MenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu';

/**
 * The 6x6 hot-spot in the top-right corner of the primary screen: hover raises the newest note,
 * click brings every note to the front, double-click creates a note, right-click shows a menu.
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
        void showCornerMenu();
      }}
    />
  );
}

async function showCornerMenu() {
  const item = (text: string, command: string) => MenuItem.new({ text, action: () => void invoke(command) });
  const menu = await Menu.new({
    items: await Promise.all([
      item('New note (double-click)', 'create_note'),
      item('Bring all notes to front (click)', 'raise_all'),
      item('Show newest note (hover)', 'focus_latest'),
      PredefinedMenuItem.new({ item: 'Separator' }),
      item('Quit Stickies', 'quit_app'),
    ]),
  });
  await menu.popup();
}
