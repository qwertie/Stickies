# Stickies

Sticky notes for Windows. Every note is a small yellow window you can type in, paste pictures and
files into, and move around. Notes are saved automatically and come back after a restart. Closing a
note archives it for 30 days in case you want it back.

Behind the scenes each note is just a folder with a Markdown file in it, so the notes are easy to
sync between PCs (they live in OneDrive by default), easy to read on a phone with Obsidian, and easy
for other programs to create. The included **morning report** does exactly that: a script runs at
4 AM, collects your Azure DevOps work, pull requests, unread email and git worktrees, has Claude
summarize them, and drops the result in as a new note.

```
Stickies/
  app/             the desktop app (Tauri 2 + React)
  morning-report/  the daily report script (PowerShell + Claude Code)
```

## Installing the app

**Easiest:** download `Stickies_x.y.z_x64-setup.exe` from the
[Releases](../../releases) page of this repository and run it. It installs for the current user
only (no admin rights needed), starts Stickies, and makes it start with Windows.

**From source:** you need [Node.js](https://nodejs.org) 22 or newer, [Rust](https://rustup.rs)
(stable, MSVC toolchain) and the Visual Studio "Desktop development with C++" build tools. Then, in
PowerShell:

```powershell
git clone https://github.com/qwertie/Stickies.git
cd Stickies\app
npm install
npm run tauri build
```

The installer appears at `app\src-tauri\target\release\bundle\nsis\`. Use `npm run tauri dev`
instead of `build` to run it with live reload while developing. Pushing a git tag such as `v0.2.0`
makes GitHub Actions build the installer and publish a release automatically.

## Using it

- **Make a note:** click **+** in a note's title bar, press **Ctrl+N**, or use the tray icon.
  New notes appear on the right side of the screen, each 200 px below the previous one.
- **Right-click a note** for everything else: colors, fonts and sizes, cut/copy/paste, undo,
  speak the note (or just the selected text), dictate, restore an archived note, open the note's
  folder, change where notes are stored, and close (archive) the note.
- **Paste anything.** Images appear inline. Files and whole folders appear as small chips.
  Double-click a chip or image to open it. Select chips along with text and copy: the files are
  put on the clipboard too, so you can paste them into Explorer or an email.
- **The tiny yellow dot** in the top-right corner of the screen: click it to bring all notes on
  top of other windows, double-click it for a new note; hovering raises the note you used last.
- **Quit** is in the right-click menu and the tray icon's menu. Stickies starts again with Windows
  (it registers itself under the current user's Run key); the tray menu's Quit does not undo that.
- **Closing a note** archives it. Right-click any note and open **Restore Archived** to get it back
  within 30 days; after that it is deleted.
- If your screen resolution changes or a monitor is unplugged, notes are pulled back on screen.
  Their saved positions are untouched, so they return to where you left them when the original
  screen arrangement comes back.

## Where the notes live

By default in `OneDrive\Stickies` if you have OneDrive, otherwise in your user folder under
`Stickies`. Right-click a note and choose **Change data folder…** to move them; existing notes move
with it. Window positions are kept per computer (in `%APPDATA%\Stickies`), so a laptop and a
desktop with different screens do not fight over layout.

```
Stickies/
  notes/
    2026-09-05-073000/
      note.md            frontmatter (color, font, fontSize, created) + Markdown body
      attachments/       pasted images, files and folders
  archive/               closed notes, same layout, with an `archived:` timestamp
```

Because it is plain Markdown with relative links, the folder is also a valid **Obsidian** vault.
To read and edit notes on a phone, open the folder in Obsidian mobile using Obsidian Sync, or the
Remotely Save plugin pointed at the same OneDrive folder. Edits made on the phone sync back and the
desktop windows update by themselves.

Any program can create a note by creating a subfolder under `notes/` containing a `note.md`. The
frontmatter is optional. That is the whole "plugin" interface, and it is how the morning report
works.

## The morning report

See [morning-report/README.md](morning-report/README.md) for setup. It needs a few one-time steps
that only you can do (an Azure DevOps token, a Microsoft sign-in, Claude Code on the PC).
