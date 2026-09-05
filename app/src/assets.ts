import { convertFileSrc } from '@tauri-apps/api/core';

/** Absolute path of the current note's folder, set once the note is loaded. */
let noteDir = '';

export function setNoteDir(dir: string) {
  noteDir = dir;
}

export function isRelativeAttachment(src: string | null | undefined): src is string {
  return !!src && src.startsWith('attachments/');
}

/** `attachments/x.png` -> an asset:// URL the webview can load. Other sources pass through. */
export function toAssetSrc(src: string): string {
  return isRelativeAttachment(src) ? convertFileSrc(`${noteDir}\\${src.replace(/\//g, '\\')}`) : src;
}

/** Inverse of toAssetSrc, so HTML copied from one of our notes round-trips. */
export function fromAssetSrc(src: string | null): string {
  if (!src) {
    return '';
  }
  const marker = src.indexOf('attachments%2F');
  if (marker >= 0) {
    return decodeURIComponent(src.slice(marker));
  }
  const slashMarker = src.indexOf('/attachments/');
  return slashMarker >= 0 ? decodeURIComponent(src.slice(slashMarker + 1)) : src;
}
