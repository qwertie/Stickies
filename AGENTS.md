Stickies is a post-it notes app that works like you'd expect, plus a separate "morning report" app that creates notes in Stickies.

## Layout

- `app/` — the desktop app: Tauri 2 (Rust, `app/src-tauri/src`) + React/TypeScript (`app/src`).
  Rust owns files, windows, tray, native clipboard and settings; TypeScript owns everything visible.
- `morning-report/` — PowerShell script + `claude -p` that writes a report note each morning. Has its
  own README with the manual setup steps.
- `scripts/Common.ps1` — helpers shared by the PowerShell scripts.

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

## Things that bit previous agents

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

## Directives

### Prime directives

- Concise, elegant, reuseable and reused code are crucial to maximize agents' productivity. Concise does NOT mean minimizing newlines. Instead, follow the generalized DRY principle: factor code to avoid repeating patterns of any kind. When adding features, actively seek out similar functionality to find opportunities for code re-use. Make the code concise via in-function refactoring during the Second Pass. Do not without approval add large ancillary code blocks to handle special cases.
- Care about Separation of Concerns and Information Hiding
- To give reviewers a simpler diff, avoid unnecessary changes during tasks (but you can ignore added/removed BOMs)
- NEVER USE `as any`. DO NOT USE `: any`. AVOID `as unknown as`.

### Style directives

- Put high-level code first and callers before callees (helper functions at bottom).
  - Put nested functions at the bottom of the outer function or of the block it is called from.
- Renaming something? If the file has that name, rename it too.
- Wrap code before column 120 and comments before column 100
- Naming:
  - Use verb phrases for names of new functions: findFoo(), not fooLookup()
  - Affixes: XIfY() = do X in case of Y, TryX() = "do X if possible" or "does not throw on failure", MaybeX() = do X if a condition to complex to describe in an IfY suffix holds; XCore() = XCore is the "core" of X() which does ancillary tasks like checking permissions
  - Use Is/Are/Get prefix on getter functions, but React Components and fast property-like queries can use a noun: `double MinimumAt(timeIndex)`
  - Indicate whether transforms are in-place or not (`reverse()` vs `getReversed()`)
  - Very long names are OK on rarely-used symbols; very oft-used words/symbols can be abbreviated
- Avoid one-liner loops/flow: spread `if (x) continue` and `for (let x of list) write(x)` over two lines
- Prefer nesting over early exit, e.g. instead of

    let c = list.find(...);
    if (!c) break;
    Change(c);
    ...

  write
  
    let c = list.find(...);
    if (c) {
        Change(c);
        ...
    }

### Other

- Use subagents more often when your task is large and context exceeds 100K tokens
- Unless you're on a dedicated temporary worktree, other agents may be working in the same tree so prefer not to stash and never switch branches when the user didn't explicitly ask for it
- By default, create a commit after completing a feature or bug fix, and offer to push it.

## Writing commit messages

Commit messages should mention

1. The task/issue number
2. *the goal* of the changes
3. *the reasons why* the changes were made (you can leave this out if the reasons are obvious. The reason to fix a bug or add a feature is usually obvious but the reason refactoring was needed is usually not and deserves a motivating explanation.)

### Structure

1. Subject line: `#<work item> <area>: <summary>`, where the optional area is a domain of substantial size e.g. TreeList (representing TreeList.tsx)
2. Superstructure: main, then Also; divide the work into a *main* work product and *ancillary* changes that the main work does not depend on, if any. Describe the main product first. Build each half independently by the rules below.

#### For each section

If it is one clear change, describe it in prose with no bullets, starting with motivation/use case, then how to use the UI if relevant, then the technical approach (mentioning real code symbols). If there are many related changes, start with an unbulleted paragraph summarizing the whole in this way, then give details as bullets. Ancillary changes follow under an `### Also` line.

When multiple bullets:

1. Plan a first draft of top-level bullets in a way that feels natural to you. Each bullet should start by saying which file(s) or class(es) were changed unless all changes are to the same file that was already mentioned.
  - For bugs, say what the bug caused, the circumstances required to trigger it, and then the bug's cause: "Bug fix: in [context], no error appeared if acquiring a lock failed because `msgBox` was uninitialized in the temporary `ViewModel`".
2. Improve the draft by merging or grouping similar/related bullets (e.g. those that share one rationale, that could only be done together, or that changed the same file).
  - Completely leave out trivial changes that don't affect the UI if their rationale is not worth recording, e.g. cleaning up `using` statements, improving a comment's wording, renaming something for clarity, or correcting a minor inefficiency.
  - Test: could a reviewer revert this bullet on its own? If not, merge it with others.
  - If there are multiple changes to the same file, consider grouping them under a `- FileName.ext:` bullet
  - Follow the usual communication directives
  - Wrap commit messages at ~75 characters
3. When a change C was done to help/enable/improve another change B, make C a child/sub-bullet of B (You can also break up a large bullet point into sub-bullets.) When C helps multiple other bullets, place C at the outer level immediately below the bullets to which it is subordinate and say something like "[Location]: To complete the N changes above it was necessary to [C]"
4. Identify bullets that are "internal", meaning changes that end-users wouldn't see, such as refactors, or fixes of theoretical non-reproduceable bugs. Show a separate section headed "Internal:" for those.
