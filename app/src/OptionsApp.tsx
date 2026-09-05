import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';

import { COLORS, FONTS, FONT_SIZES } from './menu';

/** Mirrors the Rust `Config` struct. */
export interface Settings {
  dataDir: string | null;
  defaultColor: string;
  defaultFont: string;
  defaultFontSize: number;
  archiveDays: number;
}

export function OptionsApp() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState('');

  useEffect(() => {
    void invoke<Settings>('get_settings').then(setSettings);
  }, []);

  if (!settings) {
    return null;
  }
  const update = (patch: Partial<Settings>) => setSettings({ ...settings, ...patch });

  const browse = async () => {
    const picked = await openDialog({ directory: true, defaultPath: settings.dataDir ?? undefined, title: 'Choose the Stickies data folder' });
    if (typeof picked === 'string') {
      update({ dataDir: picked });
    }
  };

  const save = async () => {
    try {
      await invoke('save_settings', { settings });
      await getCurrentWindow().close();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="options">
      <label>
        Default color
        <div className="swatches">
          {Object.entries(COLORS).map(([name, hex]) => (
            <button
              key={hex}
              type="button"
              title={name}
              className={`swatch ${hex.toUpperCase() === settings.defaultColor.toUpperCase() ? 'selected' : ''}`}
              style={{ background: hex }}
              onClick={() => update({ defaultColor: hex })}
            />
          ))}
        </div>
      </label>
      <label>
        Default font
        <select value={settings.defaultFont} onChange={(e) => update({ defaultFont: e.target.value })}>
          {FONTS.map((f) => <option key={f}>{f}</option>)}
        </select>
      </label>
      <label>
        Default size
        <select value={settings.defaultFontSize} onChange={(e) => update({ defaultFontSize: Number(e.target.value) })}>
          {FONT_SIZES.map((s) => <option key={s} value={s}>{s} pt</option>)}
        </select>
      </label>
      <label>
        Keep closed notes for
        <span className="inline">
          <input type="number" min={1} max={3650} value={settings.archiveDays} onChange={(e) => update({ archiveDays: Number(e.target.value) })} />
          days before deleting them
        </span>
      </label>
      <label>
        Data folder
        <span className="inline">
          <input type="text" value={settings.dataDir ?? ''} placeholder="(default)" onChange={(e) => update({ dataDir: e.target.value || null })} />
          <button type="button" onClick={() => void browse()}>Browse…</button>
        </span>
        <small>Changing the folder moves your notes there (if it is empty) and restarts Stickies. Window positions are per PC.</small>
      </label>
      {error && <p className="error">{error}</p>}
      <div className="buttons">
        <button type="button" onClick={() => void getCurrentWindow().close()}>Cancel</button>
        <button type="button" className="primary" onClick={() => void save()}>Save</button>
      </div>
    </div>
  );
}
