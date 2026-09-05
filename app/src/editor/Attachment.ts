import { mergeAttributes, Node } from '@tiptap/core';

/**
 * An inline, atomic chip for a non-image file or folder in the note's attachments folder.
 * Markdown form: `[name](attachments/name)`; folders carry a trailing slash. Rendered as a span,
 * not an anchor: atom nodes are non-editable, so an anchor would navigate the webview on click.
 */
export const Attachment = Node.create({
  name: 'attachment',
  group: 'inline',
  inline: true,
  atom: true,
  draggable: true,
  selectable: true,

  addAttributes() {
    return {
      href: { default: '' },
      name: { default: '' },
      isDir: { default: false },
    };
  },

  parseHTML() {
    const getAttrs = (el: HTMLElement) => ({
      href: el.getAttribute('data-href') ?? el.getAttribute('href') ?? '',
      name: el.getAttribute('data-name') ?? el.textContent ?? '',
      isDir: el.getAttribute('data-dir') === 'true',
    });
    return [{ tag: 'span[data-attachment]', getAttrs }, { tag: 'a[data-attachment]', getAttrs }];
  },

  renderHTML({ node }) {
    const { href, name, isDir } = node.attrs as { href: string; name: string; isDir: boolean };
    const attrs = mergeAttributes({
      'data-attachment': '',
      'data-href': href,
      'data-name': name,
      'data-dir': String(isDir),
      class: `attachment ${isDir ? 'attachment-dir' : 'attachment-file'}`,
      title: `${isDir ? 'Folder' : 'File'}: ${name} (double-click to open)`,
    });
    return ['span', attrs, `${isDir ? '📁' : '📄'} ${name}`];
  },

  markdownTokenName: 'link',
  priority: 1100,

  parseMarkdown(token, helpers) {
    const href = typeof token.href === 'string' ? token.href : '';
    if (!href.startsWith('attachments/')) {
      return []; // not ours: let the Link mark's handler take the token
    }
    const isDir = href.endsWith('/');
    const name = token.text || href.slice('attachments/'.length).replace(/\/$/, '');
    return helpers.createNode('attachment', { href, name, isDir });
  },

  renderMarkdown(node) {
    const { href, name } = node.attrs as { href: string; name: string };
    return `[${name}](${href})`;
  },
});
