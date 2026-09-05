# Morning report

A "plugin" for Stickies in the loosest sense: a PowerShell script that writes one note folder into
the Stickies data folder. The app's file watcher opens it as a window. Nothing in the app knows this
script exists, so any other program can produce notes the same way.

The report contains:

- Azure DevOps work items assigned to you that are not done or ready for testing
- Unassigned work items in the current sprint
- Pull requests where you are a reviewer and have not voted
- Unread Outlook mail from the last week, triaged by Claude to keep only human or important mail
- Every git worktree of the configured repos, with its branch; T3 Code worktrees are marked

## Setup

1. Copy `config.example.json` to `config.json` and adjust repos, ADO organization, project and team.
2. Create an Azure DevOps personal access token with **Work Items (Read)** and **Code (Read)** and
   store it in a user environment variable:

   ```powershell
   [Environment]::SetEnvironmentVariable('ADO_PAT', '<token>', 'User')
   ```

3. Install the Graph module and sign in once so the token cache exists for unattended runs:

   ```powershell
   Install-Module Microsoft.Graph.Authentication -Scope CurrentUser
   Connect-MgGraph -Scopes Mail.Read
   ```

   New Outlook has no COM automation surface, so Graph is the only way in. The Mail.Read delegated
   scope normally needs user consent only. If your tenant blocks it, ask an admin to consent once.

4. Make sure `claude` (Claude Code CLI) is on PATH and logged in. Without it the script still writes
   a note, just an unfiltered listing.

5. Run it by hand once, then register the scheduled task:

   ```powershell
   .\Invoke-MorningReport.ps1 -KeepRaw      # writes last.raw.json next to the script for inspection
   .\Register-MorningReportTask.ps1          # daily 04:00, wakes the PC, runs when missed
   ```

## How the timing works

The task has "Wake the computer to run this task" and "Run task as soon as possible after a
scheduled start is missed" set. So it fires at 04:00 if the PC is asleep, and at next logon if it
was off. If a data source fails (no network yet, expired token) the script exits non-zero and the
task retries up to three times, ten minutes apart. The note is written either way, with a
"Problems:" footer naming what failed.

## Files

- `Invoke-MorningReport.ps1` — gathers data, calls `claude -p`, writes the note
- `prompt.md` — the instructions Claude receives; edit to change the report's shape
- `Register-MorningReportTask.ps1` — creates the scheduled task
- `config.example.json` — template for `config.json` (which is git-ignored)
- `logs/` — one log per day (git-ignored)
