import { invoke } from '@tauri-apps/api/core';
import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu } from '@tauri-apps/api/menu';

export const COLORS: Record<string, string> = {
  Yellow: '#FFF7B1',
  Green: '#D5F5C4',
  Pink: '#FFD1DC',
  Blue: '#CFE8FF',
  Purple: '#E5D4FF',
  Orange: '#FFE0B3',
  Gray: '#E8E8E8',
  White: '#FFFFFF',
};

export const FONTS = ['Segoe UI', 'Calibri', 'Arial', 'Georgia', 'Cambria', 'Consolas', 'Comic Sans MS', 'Segoe Print'];
export const FONT_SIZES = [10, 12, 14, 16, 18, 22, 28];

interface ArchivedInfo {
  folder: string;
  archived: string;
  title: string;
}

export interface MenuActions {
  color: string;
  font: string;
  fontSize: number;
  hasSelection: boolean;
  setMeta(key: string, value: string): void;
  undo(): void;
  redo(): void;
  cut(): void;
  copy(): void;
  paste(): void;
  speak(): void;
  stopSpeaking(): void;
  dictate(): void;
  openFolder(): void;
  close(): void;
}

export async function showContextMenu(a: MenuActions) {
  const item = (text: string, action: () => void, enabled = true) => MenuItem.new({ text, enabled, action });
  const check = (text: string, checked: boolean, action: () => void) => CheckMenuItem.new({ text, checked, action });
  const separator = () => PredefinedMenuItem.new({ item: 'Separator' });

  const archived = await invoke<ArchivedInfo[]>('list_archived');
  const restoreItems = archived.length
    ? archived.map((n) =>
        item(`${formatDate(n.archived)}  ${n.title}`, () => void invoke('restore_note', { folder: n.folder })),
      )
    : [item('(nothing archived)', () => undefined, false)];

  const menu = await Menu.new({
    items: await Promise.all([
      item(a.hasSelection ? 'Speak selection' : 'Speak note', a.speak),
      item('Stop speaking', a.stopSpeaking),
      item('Dictate (Windows voice typing)\tWin+H', a.dictate),
      separator(),
      Submenu.new({
        text: 'Color',
        items: await Promise.all(
          Object.entries(COLORS).map(([name, hex]) =>
            check(name, hex.toUpperCase() === a.color.toUpperCase(), () => a.setMeta('color', hex)),
          ),
        ),
      }),
      Submenu.new({
        text: 'Font',
        items: await Promise.all([
          ...FONTS.map((f) => check(f, f === a.font, () => a.setMeta('font', f))),
          separator(),
          ...FONT_SIZES.map((s) => check(`${s} pt`, s === a.fontSize, () => a.setMeta('fontSize', String(s)))),
        ]),
      }),
      Submenu.new({ text: 'Restore Archived', items: await Promise.all(restoreItems) }),
      item('Open note folder', a.openFolder),
      item('Close note (archive)', a.close),
      separator(),
      item('Undo\tCtrl+Z', a.undo),
      item('Redo\tCtrl+Y', a.redo),
      item('Cut\tCtrl+X', a.cut, a.hasSelection),
      item('Copy\tCtrl+C', a.copy, a.hasSelection),
      item('Paste\tCtrl+V', a.paste),
      separator(),
      item('Quit Stickies', () => void invoke('quit_app')),
    ]),
  });
  await menu.popup();
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? iso
    : d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
}
