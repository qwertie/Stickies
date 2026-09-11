# AGENTS.md

Guidance for AI agents (and humans) working in this repository. The user-facing description is in
README.md; read that first for what the app does.

## Layout

- `app/` — the desktop app: Tauri 2 (Rust, `app/src-tauri/src`) + React/TypeScript (`app/src`).
  Rust owns files, windows, tray, native clipboard and settings; TypeScript owns everything visible.
- `morning-report/` — PowerShell script + `claude -p` that writes a report note each morning. Has its
  own README with the manual setup steps.
- `scripts/Common.ps1` — helpers shared by the PowerShell scripts.
- `deploy.ps1` — build, install and restart the app on this machine (see below).

## Deploying a change to the running app

After changing anything under `app/`, run from the repo root:

```powershell
.\deploy.ps1
```

It builds the Windows installer, stops the running Stickies, installs silently over the top,
relaunches, and verifies the process is back. Two to three minutes, almost all Rust linking. Use
`.\deploy.ps1 -SkipBuild` to reinstall the previous build. Do not copy `stickies.exe` around by hand
and do not run `npm run tauri dev` while the installed copy is running: both instances would share
the data folder and the single-instance lock.

Notes are safe across a deploy: every edit is written to disk within a second, and windows reopen
at their saved positions.

## Things that bite

- **Build from the real path.** `C:\Dev` may be a junction (on this machine it is a mount point onto
  `D:`). Vite refuses a project root whose realpath differs, so `deploy.ps1` resolves the junction
  first. If you run `npm` yourself, do it from `D:\Stickies\app`, not `C:\Dev\Stickies\app`.
- **Windows PowerShell 5.1** runs the scripts (it is the scheduled-task host). Save `.ps1` files with
  a UTF-8 BOM or keep them ASCII; without a BOM an em-dash decodes to a smart quote that ends a
  string. Never put Markdown ``` fences inside a double-quoted string. Read text files with
  `-Encoding UTF8`. Parse-check with `[System.Management.Automation.Language.Parser]::ParseFile`
  before running. Its `ConvertTo-Json` pretty-printer is unreadable; use `-Compress`.
- **Tauri commands that create windows must be `async fn`.** A synchronous command that builds a
  window deadlocks the IPC thread against the main thread on Windows.
- **Native popup menus block the main thread** until dismissed. Never open one by synthetic input
  from a script; an orphaned menu freezes menus and Quit until Escape is pressed.
- **The opener plugin's JS API needs a static path allow-list.** Anything that opens a path under the
  (dynamic) data folder goes through a Rust command instead.
- **The 6×6 corner window** is below Windows' minimum window size; `force_tiny_size` in
  `windows.rs` uses `SetWindowPos` with `SWP_NOSENDCHANGING` to get there.
- **Attachment chips are `<span>`s, not `<a>`s.** Atom nodes are non-editable, so an anchor inside
  one navigates the webview on click.

## Conventions

- The data folder layout (`notes/<folder>/note.md` + `attachments/`) is a public contract; other
  programs write into it. Do not add required frontmatter fields or rename folders.
- Window positions are per machine (`%APPDATA%\Stickies\layout.json`) and must never be written into
  the synced data folder.
- Settings live in `%APPDATA%\Stickies\config.json`; the Rust `Config` struct in `store.rs` and the
  `Settings` interface in `OptionsApp.tsx` must stay in sync.
- Commit after each working change and push to `origin main`; CI builds installers for Windows,
  macOS and Linux on every push. Only Windows is tested by hand.
- Everything here is public domain (Unlicense). Third-party dependencies keep their licenses.
