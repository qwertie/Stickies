/** `note.md` = YAML-ish frontmatter (flat `key: value` lines) + Markdown body. */

export const DEFAULT_COLOR = '#FFF7B1';

export type NoteMeta = Record<string, string>;

export interface ParsedNote {
  meta: NoteMeta;
  body: string;
}

export function parseNote(text: string): ParsedNote {
  const normalized = text.replace(/\r\n/g, '\n');
  const match = /^---\n([\s\S]*?)\n---\n?/.exec(normalized);
  if (!match) {
    return { meta: {}, body: normalized };
  }
  const meta: NoteMeta = {};
  for (const line of match[1].split('\n')) {
    const colon = line.indexOf(':');
    if (colon > 0) {
      const value = line.slice(colon + 1).trim();
      meta[line.slice(0, colon).trim()] = value.replace(/^"(.*)"$/, '$1');
    }
  }
  return { meta, body: normalized.slice(match[0].length).replace(/^\n/, '') };
}

export function serializeNote(meta: NoteMeta, body: string): string {
  const lines = Object.entries(meta)
    .filter(([, v]) => v !== '' && v !== undefined)
    .map(([k, v]) => `${k}: ${/^[#&*!|>'"%@`{[]/.test(v) || /:\s/.test(v) ? `"${v}"` : v}`);
  return `---\n${lines.join('\n')}\n---\n\n${body.replace(/\s+$/, '')}\n`;
}
