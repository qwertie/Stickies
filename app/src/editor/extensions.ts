import { mergeAttributes } from '@tiptap/core';
import Image from '@tiptap/extension-image';
import { Markdown } from '@tiptap/markdown';
import StarterKit from '@tiptap/starter-kit';

import { fromAssetSrc, toAssetSrc } from '../assets';
import { Attachment } from './Attachment';

/** Images keep a relative `attachments/...` src in the document; only the DOM gets the asset URL. */
const NoteImage = Image.extend({
  parseHTML() {
    return [
      {
        tag: 'img[src]',
        getAttrs: (el) => ({
          src: fromAssetSrc(el.getAttribute('src')),
          alt: el.getAttribute('alt'),
          title: el.getAttribute('title'),
        }),
      },
    ];
  },
  renderHTML({ HTMLAttributes }) {
    const src = toAssetSrc(String(HTMLAttributes.src ?? ''));
    return ['img', mergeAttributes(this.options.HTMLAttributes, HTMLAttributes, { src })];
  },
});

export function createExtensions() {
  return [
    StarterKit.configure({ link: { openOnClick: false } }),
    NoteImage.configure({ inline: true, allowBase64: false }),
    Attachment,
    Markdown,
  ];
}
