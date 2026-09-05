import { mergeAttributes, Node } from '@tiptap/core';

/**
 * An inline, atomic chip for a non-image file or folder in the note's attachments folder.
 * Markdown form: `[name](attachments/name)`; folders carry a trailing slash.
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
    return [
      {
        tag: 'a[data-attachment]',
        getAttrs: (el) => ({
          href: el.getAttribute('href') ?? '',
          name: el.getAttribute('data-name') ?? el.textContent ?? '',
          isDir: el.getAttribute('data-dir') === 'true',
        }),
      },
    ];
  },

  renderHTML({ node }) {
    const { href, name, isDir } = node.attrs as { href: string; name: string; isDir: boolean };
    const attrs = mergeAttributes({
      'data-attachment': '',
      'data-name': name,
      'data-dir': String(isDir),
      href,
      class: `attachment ${isDir ? 'attachment-dir' : 'attachment-file'}`,
      title: `${isDir ? 'Folder' : 'File'}: ${name} (double-click to open)`,
    });
    return ['a', attrs, `${isDir ? '📁' : '📄'} ${name}`];
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
