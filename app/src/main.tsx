import React from 'react';
import ReactDOM from 'react-dom/client';

import { CornerApp } from './CornerApp';
import { NoteApp } from './NoteApp';
import { OptionsApp } from './OptionsApp';
import './styles.css';

const boot = window.__STICKIES__ ?? {};
const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);

if (boot.corner) {
  document.body.classList.add('corner-body');
  root.render(<CornerApp />);
} else if (boot.options) {
  document.body.classList.add('options-body');
  root.render(<OptionsApp />);
} else if (boot.folder && boot.label) {
  root.render(
    <React.StrictMode>
      <NoteApp folder={boot.folder} />
    </React.StrictMode>,
  );
} else {
  root.render(<p style={{ padding: 8 }}>This window was opened without a note. Use the tray icon to create one.</p>);
}
