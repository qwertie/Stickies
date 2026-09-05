# Stickies

Sticky notes for Windows whose data is nothing but a folder of Markdown files, plus a morning
report generator that writes into that folder.

```
Stickies/
  app/             Tauri 2 + React desktop app
  morning-report/  PowerShell + Claude Code script that produces a daily report note
```

## The app

- Each note is a frameless 500×500 window with a thin title bar. New notes appear on the right of
  the primary screen, 15 px from the top, stepping down 200 px per note and wrapping to the top.
- Rich text (TipTap) saved as Markdown one second after the last edit. Images and files pasted into
  a note are copied into the note's `attachments/` folder and appear inline; folders too. Selecting
  them and copying puts real files on the clipboard alongside the text. Double-click opens them.
- Right-click menu: new note, undo/redo, cut/copy/paste, speak (selection or whole note), dictation,
  color, font and size, **Restore Archived** (most recently closed first), data folder commands.
- Closing a note archives it; archived notes are deleted after 30 days.
- A 6×6 px yellow hot-spot in the top-right corner of the screen: click brings every note to the
  front, hover raises the most recently created note. There is also a tray icon.
- Starts with Windows. Notes are pulled back on screen when the resolution drops or a monitor goes.
- Edits made to the files by anything else (OneDrive, Obsidian, a script) show up live.

### Data folder

Default: `%OneDrive%\Stickies` when OneDrive is set up, otherwise `%USERPROFILE%\Stickies`.
Change it from the right-click menu; the existing notes move with it. Layout (window positions) is
stored per machine under `%APPDATA%\Stickies`, not in the synced folder.

```
Stickies/
  notes/
    2026-09-05-073000/
      note.md            frontmatter (color, font, fontSize, created) + Markdown body
      attachments/       pasted images, files and folders
  archive/               closed notes, same layout, with an `archived:` timestamp
```

Because it is plain Markdown with relative links, the folder is also a valid Obsidian vault. Point
Obsidian at it on your phone (Obsidian Sync, or the Remotely Save plugin against OneDrive) to read
and edit notes there; edits sync back and the desktop windows update.

Any program can create a note by creating a subfolder under `notes/` containing `note.md`. The
frontmatter is optional. That is how the morning report works.

### Build and install

Prerequisites: Node 22+, Rust (stable, MSVC), and the Visual Studio C++ build tools.

```powershell
cd app
npm install
npm run tauri dev      # run with hot reload
npm run tauri build    # installer at src-tauri\target\release\bundle\nsis\Stickies_*_x64-setup.exe
```

Pushing a tag like `v0.1.0` makes GitHub Actions build the installer and attach it to a release,
which is the easy way to install on another PC.

## The morning report

See [morning-report/README.md](morning-report/README.md).
