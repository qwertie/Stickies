import { invoke } from '@tauri-apps/api/core';
import type { Editor, JSONContent } from '@tiptap/core';
import { DOMSerializer, type Slice } from '@tiptap/pm/model';
import type { EditorView } from '@tiptap/pm/view';

import { isRelativeAttachment } from '../assets';

export interface ImportedFile {
  rel: string;
  name: string;
  isDir: boolean;
  isImage: boolean;
}

interface PasteContent {
  text: string | null;
  files: ImportedFile[];
}

export function nodesFor(files: ImportedFile[]): JSONContent[] {
  const nodes: JSONContent[] = [];
  for (const f of files) {
    if (nodes.length > 0) {
      nodes.push({ type: 'text', text: ' ' });
    }
    if (f.isImage) {
      nodes.push({ type: 'image', attrs: { src: f.rel, alt: f.name } });
    } else {
      nodes.push({ type: 'attachment', attrs: { href: f.isDir ? `${f.rel}/` : f.rel, name: f.name, isDir: f.isDir } });
    }
  }
  return nodes;
}

/**
 * Ctrl+V. Files and folders on the clipboard (Explorer copy, or one of our own copies) are imported
 * into the note's attachments; screenshots arrive as image blobs. Text and HTML fall through to
 * TipTap's default handling.
 */
export function handlePaste(editor: Editor, folder: string, event: ClipboardEvent): boolean {
  const data = event.clipboardData;
  if (!data) {
    return false;
  }
  const hasFiles = data.types.includes('Files');
  const imageItems = Array.from(data.items).filter((i) => i.kind === 'file' && i.type.startsWith('image/'));
  if (!hasFiles && imageItems.length === 0) {
    return false;
  }
  const html = data.getData('text/html');
  const imageBlobs = imageItems.map((i) => i.getAsFile()).filter((f): f is File => !!f);
  void pasteFilesAsync(editor, folder, html, imageBlobs);
  return true;
}

async function pasteFilesAsync(editor: Editor, folder: string, html: string, imageBlobs: File[]) {
  let imported = await invoke<ImportedFile[]>('import_clipboard_files', { folder });
  if (imported.length === 0) {
    imported = [];
    for (const blob of imageBlobs) {
      const ext = blob.type.split('/')[1] ?? 'png';
      const name = blob.name && blob.name !== 'image.png' ? blob.name : `pasted-${timestamp()}.${ext}`;
      const bytes = Array.from(new Uint8Array(await blob.arrayBuffer()));
      imported.push(await invoke<ImportedFile>('save_attachment', { folder, name, bytes }));
    }
  }
  if (imported.length === 0) {
    return;
  }
  if (html.includes('data-attachment') || html.includes('attachments')) {
    editor.commands.insertContent(rewriteCopiedHtml(html, imported), { contentType: 'html' });
  } else {
    editor.commands.insertContent(nodesFor(imported));
  }
}

/** Context-menu Paste: the browser paste event is unavailable, so the Rust side reads the clipboard. */
export async function pasteFromMenu(editor: Editor, folder: string) {
  const content = await invoke<PasteContent>('read_clipboard_for_paste', { folder });
  if (content.files.length > 0) {
    editor.commands.insertContent(nodesFor(content.files));
  } else if (content.text) {
    editor.commands.insertContent(content.text, { contentType: 'markdown' });
  }
}

/** Copies selection as text + HTML + a real file list of any attachments inside it. */
export function copySelection(view: EditorView, folder: string, cut: boolean): boolean {
  const slice = view.state.selection.content();
  if (slice.size === 0) {
    return false;
  }
  void invoke('copy_to_clipboard', {
    folder,
    text: textOf(slice),
    html: htmlOf(view, slice),
    attachments: attachmentsIn(slice),
  });
  if (cut) {
    view.dispatch(view.state.tr.deleteSelection());
  }
  return true;
}

function textOf(slice: Slice): string {
  return slice.content.textBetween(0, slice.content.size, '\n\n', (node) => {
    if (node.type.name === 'attachment') {
      return String(node.attrs.name);
    }
    if (node.type.name === 'image') {
      return String(node.attrs.alt ?? '');
    }
    return '';
  });
}

function htmlOf(view: EditorView, slice: Slice): string {
  const div = document.createElement('div');
  div.appendChild(DOMSerializer.fromSchema(view.state.schema).serializeFragment(slice.content));
  return div.innerHTML;
}

function attachmentsIn(slice: Slice): string[] {
  const rels: string[] = [];
  slice.content.descendants((node) => {
    const rel = node.type.name === 'attachment' ? String(node.attrs.href) : node.type.name === 'image' ? String(node.attrs.src) : '';
    if (isRelativeAttachment(rel)) {
      rels.push(rel.replace(/\/$/, ''));
    }
  });
  return rels;
}

/**
 * HTML copied from a note refers to that note's attachments. After importing the files into this
 * note (which may rename duplicates), point the copied nodes at the imported files, in order.
 */
function rewriteCopiedHtml(html: string, imported: ImportedFile[]): string {
  const doc = new DOMParser().parseFromString(html, 'text/html');
  const refs = Array.from(doc.querySelectorAll<HTMLElement>('a[data-attachment], img'));
  let i = 0;
  for (const el of refs) {
    const file = imported[i];
    if (!file) {
      break;
    }
    if (el instanceof HTMLImageElement) {
      el.src = file.rel;
    } else {
      el.setAttribute('href', file.isDir ? `${file.rel}/` : file.rel);
      el.setAttribute('data-name', file.name);
    }
    i += 1;
  }
  return doc.body.innerHTML;
}

function timestamp(): string {
  return new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
}
