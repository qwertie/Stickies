# Morning report

A PowerShell script that writes one note into the Stickies data folder every morning. The app's
file watcher notices the new folder and opens it as a window. Nothing in the app knows this script
exists; any other program can produce notes the same way.

The report contains:

- Azure DevOps work items assigned to you that are not done or ready for testing
- Unassigned work items in the current sprint ("up for grabs")
- Pull requests where you are a reviewer and have not voted yet
- Pipeline runs that failed in the last few days (`ado.failedBuildDays`) and concern you: the branch
  name contains the number of one of your work items, a worktree on this machine has that branch
  checked out, or you requested the build
- Unread Outlook mail from the last week, triaged by Claude so only mail from real people, or mail
  that clearly needs you, is listed
- Every git worktree of your configured repositories with its branch; T3 Code worktrees are marked

## One-time setup

The script talks to three services on your behalf, and each needs you to prove who you are once.
Nothing here can be automated by an agent because each step is a login.

### 1. Copy and edit the config

```powershell
cd <repo>\morning-report
Copy-Item config.example.json config.json
```

Open `config.json` and check `repos` (the git repositories whose worktrees you want listed),
`ado.organization` and `ado.project` (from your Azure DevOps URL,
`https://dev.azure.com/<organization>/<project>`), and `ado.doneStates` (the states you consider
finished). Leave `ado.team` as `null` to use the project's default team, which is what determines
"the current sprint". `config.json` is git-ignored.

### 2. Azure DevOps: create a Personal Access Token (PAT)

**Purpose:** the script reads your work items and pull requests through the Azure DevOps REST API.
The API needs a credential, and a PAT is a password-like token that is limited to the permissions
you choose and expires on a date you choose. It never leaves your PC.

1. Sign in to `https://dev.azure.com/<organization>`.
2. Click the **User settings** icon (top right, next to your avatar) and choose
   **Personal access tokens**.
3. Click **+ New Token**. Give it a name such as `Stickies morning report`, choose your
   organization, and set an expiration (up to one year; you will repeat this step when it expires).
4. Under **Scopes**, choose **Custom defined**, then tick **Work Items: Read**, **Code: Read** and
   **Build: Read**. Nothing else. Click **Create**.
5. Copy the token now; Azure DevOps will not show it again.
6. Store it in a user environment variable so the script can read it. In PowerShell:

   ```powershell
   [Environment]::SetEnvironmentVariable('ADO_PAT', '<paste token here>', 'User')
   ```

   Open a new PowerShell window afterwards; existing windows do not see new variables. If you
   prefer a different variable name, change `ado.patEnvVar` in `config.json`.

### 3. Outlook mail: sign in to Microsoft Graph once

**Purpose:** New Outlook cannot be read by other programs on the PC, so the script asks Microsoft's
cloud API (Microsoft Graph) for your unread mail instead. That requires one interactive sign-in;
after it, Windows keeps a refresh token so the 4 AM run works without you. The script asks only for
`Mail.Read`, which means it can read mail but never send, move or delete anything.

```powershell
Install-Module Microsoft.Graph.Authentication -Scope CurrentUser   # if not already installed
Connect-MgGraph -Scopes Mail.Read
```

A browser window opens for your work account. If your organization blocks user consent you will
see an "approval required" page; ask an administrator to approve the "Microsoft Graph Command Line
Tools" application with the `Mail.Read` permission.

The email section can be switched off with `"enabled": false` under `email` in `config.json`.

### 4. Claude Code

**Purpose:** the raw data is turned into a short, readable report by Claude, which also does the
"is this email from a human and does it need me?" judgement. This uses the
[Claude Code](https://claude.com/claude-code) command-line tool, `claude`, which must be installed
and logged in on this PC. Without it the script still writes a note, just as an unfiltered listing.
`claudeModel` in `config.json` chooses the model; `sonnet` is fast and cheap enough for this.

### 5. Try it, then schedule it

```powershell
.\Invoke-MorningReport.ps1 -KeepRaw      # writes the note now; keeps last.raw.json for inspection
.\Register-MorningReportTask.ps1          # creates the daily 04:00 scheduled task
```

If a step above was skipped, the note still appears but ends with a "Problems:" line naming what
failed, so you can tell at a glance what is left to set up.

## How the timing works

The scheduled task has "Wake the computer to run this task" and "Run task as soon as possible
after a scheduled start is missed" enabled. So it fires at 04:00 if the PC is asleep, and at your
next logon if the PC was off. It runs as your user only while you are logged on (the lock screen
counts), because that is the only way it can reach your Graph sign-in, your Claude login and the
`ADO_PAT` variable. If a data source fails (no network yet, expired token), the script exits with an
error and the task retries up to three times, ten minutes apart.

## Files

- `Invoke-MorningReport.ps1` — gathers data, calls `claude -p`, writes the note
- `prompt.md` — the instructions Claude receives; edit this to change the report's shape
- `Register-MorningReportTask.ps1` — creates or updates the scheduled task
- `config.example.json` — template for `config.json`
- `logs/` — one log file per day (git-ignored)
