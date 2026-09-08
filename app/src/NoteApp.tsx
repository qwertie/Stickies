import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Editor } from '@tiptap/core';
import { EditorContent, useEditor, useEditorState } from '@tiptap/react';
import { useCallback, useEffect, useRef, useState } from 'react';

import { setNoteDir } from './assets';
import { copySelection, dropFiles, handlePaste, openAttachmentAt, pasteFromMenu } from './editor/clipboard';
import { createExtensions } from './editor/extensions';
import { showContextMenu } from './menu';
import { DEFAULT_COLOR, parseNote, serializeNote, type NoteMeta } from './noteFile';
import { speak, stopSpeaking } from './speech';

const SAVE_DELAY_MS = 1000;
const LAYOUT_DELAY_MS = 500;

interface NoteLoad {
  content: string;
  dataDir: string;
  noteDir: string;
  defaults: { defaultColor: string; defaultFont: string; defaultFontSize: number };
}

interface Defaults {
  color: string;
  font: string;
  fontSize: number;
}

interface NoteChanged {
  folder: string;
  content: string;
}

export function NoteApp({ folder }: { folder: string }) {
  const [meta, setMetaState] = useState<NoteMeta>({});
  const [defaults, setDefaults] = useState<Defaults>({ color: DEFAULT_COLOR, font: 'Segoe UI', fontSize: 14 });
  const [loaded, setLoaded] = useState(false);
  const metaRef = useRef<NoteMeta>({});
  const lastSaved = useRef('');
  const saveTimer = useRef<number | undefined>(undefined);
  const editorRef = useRef<Editor | null>(null);

  const editor = useEditor({
    extensions: createExtensions(),
    autofocus: 'end',
    editorProps: {
      attributes: { class: 'note-editor', spellcheck: 'true' },
      handlePaste: (_view, event) => (editorRef.current ? handlePaste(editorRef.current, folder, event) : false),
      handleDOMEvents: {
        copy: (view, event) => copySelection(view, folder, false) && preventDefault(event),
        cut: (view, event) => copySelection(view, folder, true) && preventDefault(event),
        dblclick: (_view, event) => openAttachmentAt(event.target, folder) && preventDefault(event),
      },
    },
    onUpdate: () => scheduleSave(),
  });
  editorRef.current = editor;

  const save = useCallback(() => {
    window.clearTimeout(saveTimer.current);
    saveTimer.current = undefined;
    if (editor && loaded) {
      const content = serializeNote(metaRef.current, editor.getMarkdown());
      if (content !== lastSaved.current) {
        lastSaved.current = content;
        void invoke('write_note', { folder, content });
      }
    }
  }, [editor, folder, loaded]);

  const scheduleSave = useCallback(() => {
    window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(save, SAVE_DELAY_MS);
  }, [save]);

  const applyContent = useCallback(
    (text: string) => {
      const parsed = parseNote(text);
      metaRef.current = parsed.meta;
      setMetaState(parsed.meta);
      editor?.commands.setContent(parsed.body, { contentType: 'markdown', emitUpdate: false });
      lastSaved.current = text;
    },
    [editor],
  );

  useEffect(() => {
    if (!editor) {
      return;
    }
    let cancelled = false;
    void invoke<NoteLoad>('load_note', { folder }).then((load) => {
      if (!cancelled) {
        setNoteDir(load.noteDir);
        setDefaults({ color: load.defaults.defaultColor, font: load.defaults.defaultFont, fontSize: load.defaults.defaultFontSize });
        applyContent(load.content);
        setLoaded(true);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [editor, folder, applyContent]);

  useEffect(() => {
    const unlisten = listen<NoteChanged>('note-changed', (e) => {
      const external = e.payload.folder === folder && e.payload.content !== lastSaved.current;
      if (external && saveTimer.current === undefined) {
        applyContent(e.payload.content);
      }
    });
    return () => void unlisten.then((f) => f());
  }, [folder, applyContent]);

  useEffect(() => {
    const win = getCurrentWindow();
    let timer: number | undefined;
    const persist = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(async () => {
        const scale = await win.scaleFactor();
        const pos = (await win.outerPosition()).toLogical(scale);
        const size = (await win.outerSize()).toLogical(scale);
        void invoke('save_layout', {
          folder,
          x: Math.round(pos.x),
          y: Math.round(pos.y),
          w: Math.round(size.width),
          h: Math.round(size.height),
        });
      }, LAYOUT_DELAY_MS);
    };
    const unlisteners = [win.onMoved(persist), win.onResized(persist)];
    return () => void Promise.all(unlisteners).then((fs) => fs.forEach((f) => f()));
  }, [folder]);

  useEffect(() => {
    const unlisten = listen('save-now', save);
    return () => void unlisten.then((f) => f());
  }, [save]);

  useEffect(() => {
    if (!editor) {
      return;
    }
    const unlisten = getCurrentWebview().onDragDropEvent(async (e) => {
      if (e.payload.type === 'drop' && e.payload.paths.length > 0) {
        const scale = await getCurrentWindow().scaleFactor();
        const p = e.payload.position.toLogical(scale);
        const pos = editor.view.posAtCoords({ left: p.x, top: p.y })?.pos ?? null;
        await dropFiles(editor, folder, e.payload.paths, pos);
      }
    });
    return () => void unlisten.then((f) => f());
  }, [editor, folder]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        void invoke('create_note');
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const setMeta = (key: string, value: string) => {
    metaRef.current = { ...metaRef.current, [key]: value };
    setMetaState(metaRef.current);
    save();
  };

  const closeNote = () => {
    save();
    void invoke('close_note', { folder });
  };

  const onContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    if (!editor) {
      return;
    }
    const { from, to } = editor.state.selection;
    const selectedText = editor.state.doc.textBetween(from, to, '\n');
    void showContextMenu({
      color: meta.color ?? defaults.color,
      font: meta.font ?? defaults.font,
      fontSize: Number(meta.fontSize ?? defaults.fontSize),
      hasSelection: from !== to,
      setMeta,
      undo: () => editor.commands.undo(),
      redo: () => editor.commands.redo(),
      cut: () => copySelection(editor.view, folder, true),
      copy: () => copySelection(editor.view, folder, false),
      paste: () => void pasteFromMenu(editor, folder),
      speak: () => speak(selectedText || editor.getText()),
      stopSpeaking,
      openFolder: () => void invoke('open_in_explorer', { folder, rel: null }),
      close: closeNote,
    });
  };

  const color = meta.color ?? defaults.color;
  const style: React.CSSProperties = {
    background: color,
    fontFamily: meta.font ?? defaults.font,
    fontSize: `${meta.fontSize ?? defaults.fontSize}pt`,
  };

  return (
    <div className="note" style={style} onContextMenu={onContextMenu}>
      <div className="titlebar" data-tauri-drag-region style={{ background: darken(color) }}>
        <button className="titlebar-btn" title="New note (Ctrl+N)" onClick={() => void invoke('create_note')}>+</button>
        <FormatButtons editor={editor} />
        <span className="titlebar-title" data-tauri-drag-region />

        <button className="titlebar-btn" title="Close (archive for 30 days)" onClick={closeNote}>×</button>
      </div>
      <EditorContent editor={editor} className="editor-host" />
    </div>
  );
}

const FORMATS = [
  { name: 'bold', label: 'B', title: 'Bold (Ctrl+B)', style: { fontWeight: 700 } },
  { name: 'italic', label: 'I', title: 'Italic (Ctrl+I)', style: { fontStyle: 'italic' } },
  { name: 'strike', label: 'S', title: 'Strikethrough (Ctrl+Shift+S)', style: { textDecoration: 'line-through' } },
  { name: 'bulletList', label: '•', title: 'Bulleted list (Ctrl+Shift+8)', style: {} },
  { name: 'orderedList', label: '1.', title: 'Numbered list (Ctrl+Shift+7)', style: {} },
  { name: 'blockquote', label: '❝', title: 'Quotation (Ctrl+Shift+B)', style: {} },
] as const;

type FormatName = (typeof FORMATS)[number]['name'];

const TOGGLE: Record<FormatName, (e: Editor) => boolean> = {
  bold: (e) => e.chain().focus().toggleBold().run(),
  italic: (e) => e.chain().focus().toggleItalic().run(),
  strike: (e) => e.chain().focus().toggleStrike().run(),
  bulletList: (e) => e.chain().focus().toggleBulletList().run(),
  orderedList: (e) => e.chain().focus().toggleOrderedList().run(),
  blockquote: (e) => e.chain().focus().toggleBlockquote().run(),
};

/** Formatting toggles in the title bar; highlighted when the selection already has the format. */
function FormatButtons({ editor }: { editor: Editor | null }) {
  const active = useEditorState({
    editor,
    selector: (ctx) => Object.fromEntries(FORMATS.map((f) => [f.name, ctx.editor?.isActive(f.name) ?? false])),
  });
  return (
    <span className="titlebar-tools">
      {FORMATS.map((f) => (
        <button
          key={f.name}
          className={`titlebar-btn ${active?.[f.name] ? 'active' : ''}`}
          title={f.title}
          style={f.style}
          onMouseDown={(e) => e.preventDefault()} // keep the editor's selection
          onClick={() => editor && TOGGLE[f.name](editor)}
        >
          {f.label}
        </button>
      ))}
    </span>
  );
}

function preventDefault(event: Event): boolean {
  event.preventDefault();
  return true;
}

function darken(hex: string): string {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex);
  if (!m) {
    return 'rgba(0,0,0,0.08)';
  }
  const [r, g, b] = [m[1], m[2], m[3]].map((c) => Math.max(0, parseInt(c, 16) - 28));
  return `rgb(${r},${g},${b})`;
}
