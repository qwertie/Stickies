<#
.SYNOPSIS
  Builds the morning report and drops it into the Stickies data folder as a note.

.DESCRIPTION
  Gathers Azure DevOps work items and pull requests, unread Outlook mail via Microsoft Graph, and
  git worktrees; then asks Claude Code (claude -p) to write a short Markdown report. Falls back to a
  plain listing when Claude is unavailable. The result is written to
  <dataDir>\notes\<yyyy-MM-dd>-morning-report\note.md, which the Stickies app picks up automatically.

  Exit code is non-zero when a data source failed, so Task Scheduler can retry.
#>
[CmdletBinding()]
param(
    [string]$ConfigPath = "$PSScriptRoot\config.json",
    [switch]$NoClaude,
    [switch]$KeepRaw,
    [switch]$SimulateAuthFailure
)

$ErrorActionPreference = 'Stop'
$script:Failures = @()
$script:ClaudeNeedsLogin = $false
$logDir = Join-Path $PSScriptRoot 'logs'
New-Item -ItemType Directory -Force $logDir | Out-Null
$logFile = Join-Path $logDir ("{0:yyyy-MM-dd}.log" -f (Get-Date))

function Write-Log([string]$Message) {
    $line = "{0:HH:mm:ss} {1}" -f (Get-Date), $Message
    Write-Host $line
    Add-Content -Path $logFile -Value $line -Encoding utf8
}

function Add-Failure([string]$Source, [string]$Reason) {
    $script:Failures += "$Source`: $Reason"
    Write-Log "FAILED $Source`: $Reason"
}

function Expand-EnvPath([string]$Path) {
    if ($Path) { [Environment]::ExpandEnvironmentVariables($Path) } else { $Path }
}

# ---------------------------------------------------------------- configuration
if (-not (Test-Path $ConfigPath)) {
    throw "Config not found at $ConfigPath. Copy config.example.json to config.json and edit it."
}
$config = Get-Content $ConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json

function Get-DataDir {
    if ($config.dataDir) { return Expand-EnvPath $config.dataDir }
    $appConfig = Join-Path $env:APPDATA 'Stickies\config.json'
    if (Test-Path $appConfig) {
        $dir = (Get-Content $appConfig -Raw | ConvertFrom-Json).dataDir
        if ($dir) { return $dir }
    }
    if ($env:OneDrive -and (Test-Path $env:OneDrive)) { return Join-Path $env:OneDrive 'Stickies' }
    return Join-Path $env:USERPROFILE 'Stickies'
}

# ---------------------------------------------------------------- worktrees
function Get-Worktrees {
    $t3Root = Expand-EnvPath $config.t3WorktreesRoot
    $result = @()
    foreach ($repo in $config.repos) {
        if (-not (Test-Path $repo)) { continue }
        $lines = & git -C $repo worktree list --porcelain 2>$null
        $current = $null
        foreach ($line in $lines) {
            if ($line -like 'worktree *') {
                $current = [ordered]@{ path = $line.Substring(9); branch = '(detached)'; repo = (Split-Path $repo -Leaf); isT3 = $false }
            } elseif ($line -like 'branch *') {
                $current.branch = $line.Substring(7) -replace '^refs/heads/', ''
            } elseif ($line -eq '' -and $current) {
                $current.isT3 = $t3Root -and $current.path -like "$t3Root*"
                $result += [pscustomobject]$current
                $current = $null
            }
        }
        if ($current) { $result += [pscustomobject]$current }
    }
    return $result
}

# ---------------------------------------------------------------- Azure DevOps
# Invoke-RestMethod's error message for a 4xx drops the response body, which is where Azure DevOps
# explains what it objected to. This wrapper keeps it, along with the URL.
function Invoke-Ado([string]$Uri, [hashtable]$Headers, [string]$Method = 'Get', $Body = $null) {
    # Invoke-RestMethod's default is no timeout at all. dev.azure.com has several addresses and one
    # of them has been seen swallowing SYNs for minutes; a bounded wait lets the retry loop take over.
    try {
        if ($Body) {
            Invoke-RestMethod $Uri -Method $Method -Headers $Headers -ContentType 'application/json' -Body $Body -TimeoutSec 30
        } else {
            Invoke-RestMethod $Uri -Method $Method -Headers $Headers -TimeoutSec 30
        }
    } catch [System.Net.WebException] {
        $detail = ''
        if ($_.Exception.Response) {
            $reader = New-Object IO.StreamReader($_.Exception.Response.GetResponseStream())
            $detail = $reader.ReadToEnd()
            try { $detail = ($detail | ConvertFrom-Json).message } catch {}
        }
        throw "$($_.Exception.Message) for $Uri`: $detail"
    }
}

function Get-AdoData([array]$Worktrees) {
    $ado = $config.ado
    # A variable set in another window (or by SetEnvironmentVariable 'User') is not in this
    # process's environment yet, so fall back to the registry copy.
    $pat = [Environment]::GetEnvironmentVariable($ado.patEnvVar)
    if (-not $pat) { $pat = [Environment]::GetEnvironmentVariable($ado.patEnvVar, 'User') }
    if (-not $pat) {
        Add-Failure 'Azure DevOps' "environment variable $($ado.patEnvVar) is not set"
        return $null
    }
    $headers = @{ Authorization = 'Basic ' + [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes(":$pat")) }
    $org = "https://dev.azure.com/$($ado.organization)"
    $project = [uri]::EscapeDataString($ado.project)
    $api = 'api-version=7.1'

    # connectionData is an old endpoint that rejects api-version=7.1 with a bare 400.
    $me = (Invoke-Ado "$org/_apis/connectionData?api-version=7.1-preview.1" $headers).authenticatedUser
    $team = $ado.team
    if (-not $team) {
        $team = (Invoke-Ado "$org/_apis/projects/$project`?$api" $headers).defaultTeam.name
    }
    $teamPath = "$org/$project/$([uri]::EscapeDataString($team))"

    $notDone = ($ado.doneStates | ForEach-Object { "'$_'" }) -join ', '
    $types = ($ado.workItemTypes | ForEach-Object { "'$_'" }) -join ', '
    $baseWhere = "[System.WorkItemType] IN ($types) AND [System.State] NOT IN ($notDone)"

    function Invoke-Wiql([string]$Where) {
        $body = @{ query = "SELECT [System.Id] FROM WorkItems WHERE $Where ORDER BY [Microsoft.VSTS.Common.Priority] ASC, [System.ChangedDate] DESC" } | ConvertTo-Json
        $ids = (Invoke-Ado "$teamPath/_apis/wit/wiql?$api&`$top=100" $headers Post $body).workItems.id
        if (-not $ids) { return @() }
        $fields = 'System.Id,System.Title,System.State,System.WorkItemType,System.IterationPath,Microsoft.VSTS.Common.Priority,System.ChangedDate,System.AssignedTo'
        $items = (Invoke-Ado "$org/$project/_apis/wit/workitems?ids=$($ids -join ',')&fields=$fields&$api" $headers).value
        return @($items | ForEach-Object {
            [pscustomobject]@{
                id = $_.id; title = $_.fields.'System.Title'; state = $_.fields.'System.State'
                type = $_.fields.'System.WorkItemType'; iteration = $_.fields.'System.IterationPath'
                priority = $_.fields.'Microsoft.VSTS.Common.Priority'; changed = $_.fields.'System.ChangedDate'
                url = "$org/$project/_workitems/edit/$($_.id)"
            }
        })
    }

    $mine = Invoke-Wiql "$baseWhere AND [System.AssignedTo] = @Me"
    $upForGrabs = Invoke-Wiql "$baseWhere AND [System.IterationPath] = @CurrentIteration AND [System.AssignedTo] = ''"

    $prs = (Invoke-Ado "$org/$project/_apis/git/pullrequests?searchCriteria.status=active&searchCriteria.reviewerId=$($me.id)&$api" $headers).value
    $awaiting = @($prs | Where-Object {
        $myVote = ($_.reviewers | Where-Object { $_.id -eq $me.id } | Select-Object -First 1).vote
        $myVote -eq 0 -and $_.createdBy.id -ne $me.id
    } | ForEach-Object {
        [pscustomobject]@{
            id = $_.pullRequestId; title = $_.title; repo = $_.repository.name; author = $_.createdBy.displayName
            created = $_.creationDate; isDraft = $_.isDraft
            url = "$org/$project/_git/$([uri]::EscapeDataString($_.repository.name))/pullrequest/$($_.pullRequestId)"
        }
    })

    # Failed pipeline runs that have some connection to me: the branch names one of my work items,
    # a worktree on this machine has the branch checked out, or I requested the build. Only the
    # latest completed run per pipeline+branch counts, so a failure followed by a fix is not listed.
    $failedBuilds = @()
    if ([int]$ado.failedBuildDays -gt 0) {
        $minTime = (Get-Date).AddDays(-[int]$ado.failedBuildDays).ToUniversalTime().ToString('o')
        $builds = (Invoke-Ado "$org/$project/_apis/build/builds?statusFilter=completed&minTime=$minTime&queryOrder=finishTimeDescending&`$top=500&$api" $headers).value
        $myIds = @($mine | ForEach-Object { [string]$_.id })
        $seen = @{}
        foreach ($b in $builds) {
            $branch = $b.sourceBranch -replace '^refs/heads/', ''
            $key = "$($b.definition.id)|$branch"
            if ($seen.ContainsKey($key)) { continue }
            $seen[$key] = $true
            if ($b.result -ne 'failed') { continue }
            $reasons = @()
            $ticketHits = @([regex]::Matches($branch, '\d{3,6}') | ForEach-Object { $_.Value } | Where-Object { $myIds -contains $_ })
            if ($ticketHits) { $reasons += "branch names my work item #$($ticketHits -join ', #')" }
            $wt = $Worktrees | Where-Object { $_.branch -eq $branch } | Select-Object -First 1
            if ($wt) { $reasons += "checked out in worktree $($wt.path)" }
            if ($b.requestedFor.id -eq $me.id) { $reasons += 'I requested the build' }
            if ($reasons) {
                $failedBuilds += [pscustomobject]@{
                    pipeline = $b.definition.name; buildNumber = $b.buildNumber; branch = $branch
                    finished = $b.finishTime; requestedBy = $b.requestedFor.displayName
                    reason = ($reasons -join '; '); url = $b._links.web.href
                }
            }
        }
    }

    return [pscustomobject]@{
        me = $me.providerDisplayName; myWorkItems = $mine; upForGrabs = $upForGrabs
        prsAwaitingMyReview = $awaiting; failedBuilds = $failedBuilds
    }
}

# ---------------------------------------------------------------- Outlook via Graph
function Get-EmailData {
    $mail = $config.email
    if (-not $mail.enabled) { return $null }
    if (-not (Get-Module -ListAvailable Microsoft.Graph.Authentication)) {
        Add-Failure 'Email' 'Microsoft.Graph.Authentication module not installed (Install-Module Microsoft.Graph.Authentication -Scope CurrentUser)'
        return $null
    }
    Import-Module Microsoft.Graph.Authentication
    try {
        Connect-MgGraph -Scopes 'Mail.Read' -NoWelcome | Out-Null
    } catch {
        Add-Failure 'Email' "Connect-MgGraph failed: $($_.Exception.Message). Run the script once interactively to sign in."
        return $null
    }
    $since = (Get-Date).AddDays(-[int]$mail.days).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    $select = 'subject,from,toRecipients,receivedDateTime,bodyPreview,inferenceClassification,importance,webLink,hasAttachments'
    $uri = "https://graph.microsoft.com/v1.0/me/messages?`$filter=isRead eq false and receivedDateTime ge $since&`$select=$select&`$orderby=receivedDateTime desc&`$top=$($mail.maxMessages)"
    $messages = (Invoke-MgGraphRequest -Method GET -Uri $uri -OutputType PSObject).value
    $myAddress = (Get-MgContext).Account

    $patterns = $mail.ignoreSenderPatterns
    $domains = $mail.ignoreDomains
    $kept = @(); $skipped = 0
    foreach ($m in $messages) {
        $addr = $m.from.emailAddress.address
        $lower = "$addr".ToLowerInvariant()
        $domain = ($lower -split '@')[-1]
        $automated = ($patterns | Where-Object { $lower -like "*$_*" }) -or ($domains -contains $domain)
        if ($automated) { $skipped++; continue }
        $kept += [pscustomobject]@{
            from = $m.from.emailAddress.name; address = $addr; subject = $m.subject
            received = $m.receivedDateTime; preview = $m.bodyPreview
            focused = ($m.inferenceClassification -eq 'focused'); importance = $m.importance
            addressedToMe = [bool]($m.toRecipients | Where-Object { $_.emailAddress.address -eq $myAddress })
            hasAttachments = $m.hasAttachments; link = $m.webLink
        }
    }
    return [pscustomobject]@{ account = $myAddress; unreadDays = $mail.days; messages = $kept; preFilteredOut = $skipped }
}

# ---------------------------------------------------------------- report generation
function Invoke-Claude([string]$RawJson) {
    $claude = Get-Command claude -ErrorAction SilentlyContinue
    if (-not $claude) {
        Add-Failure 'Claude' 'claude CLI not found on PATH'
        return $null
    }
    # Logins expire. Check first so the failure is explained instead of surfacing as a cryptic
    # exit code, and so the note can offer a one-click fix.
    $status = if ($SimulateAuthFailure) { '{"loggedIn": false}' } else { (& $claude.Source auth status --json 2>&1) -join "`n" }
    $loggedIn = $env:CLAUDE_CODE_OAUTH_TOKEN -or $env:ANTHROPIC_API_KEY -or ($status -match '"loggedIn"\s*:\s*true')
    if (-not $loggedIn) {
        $script:ClaudeNeedsLogin = $true
        Add-Failure 'Claude' 'not signed in (login expired)'
        return $null
    }
    # claude speaks UTF-8; Windows PowerShell pipes re-decode native output as ANSI and mangle
    # anything beyond ASCII (em-dashes became "â€”"). Go through files instead, which cmd.exe
    # passes byte-for-byte.
    $fence = [string][char]96 * 3
    $prompt = (Get-Content "$PSScriptRoot\prompt.md" -Raw -Encoding UTF8) + "`n`n" + $fence + "json`n" + $RawJson + "`n" + $fence
    $tmp = Join-Path $env:TEMP ("stickies-report-" + [guid]::NewGuid().ToString('N'))
    $utf8 = [Text.UTF8Encoding]::new($false)
    [IO.File]::WriteAllText("$tmp.in.md", $prompt, $utf8)
    $effort = if ($config.claudeEffort) { "--effort $($config.claudeEffort)" } else { '' }
    $cmdLine = '"{0}" -p --output-format text --model {1} {2} < "{3}.in.md" > "{3}.out.md" 2> "{3}.err.txt"' -f $claude.Source, $config.claudeModel, $effort, $tmp
    & cmd.exe /d /c $cmdLine
    $exit = $LASTEXITCODE
    $output = if (Test-Path "$tmp.out.md") { [IO.File]::ReadAllText("$tmp.out.md", $utf8) } else { '' }
    $errText = if (Test-Path "$tmp.err.txt") { [IO.File]::ReadAllText("$tmp.err.txt", $utf8) } else { '' }
    Remove-Item "$tmp.*" -ErrorAction SilentlyContinue
    if ($exit -ne 0 -or -not $output.Trim()) {
        $output = "$output`n$errText"
        if ("$output" -match 'log ?in|authenticat|Invalid API key|OAuth|token.*(expired|invalid)|401') {
            $script:ClaudeNeedsLogin = $true
            Add-Failure 'Claude' 'sign-in rejected (login expired)'
        } else {
            Add-Failure 'Claude' "claude -p exited with $exit`: $($output.Trim())"
        }
        return $null
    }
    return $output
}

function Format-Fallback($raw) {
    $sb = [Text.StringBuilder]::new()
    if (-not $script:ClaudeNeedsLogin) { [void]$sb.AppendLine("_Claude was unavailable; this is the unfiltered listing._`n") }
    if ($raw.ado) {
        [void]$sb.AppendLine('## My work items')
        foreach ($w in $raw.ado.myWorkItems) { [void]$sb.AppendLine("- **#$($w.id)** $($w.title) ($($w.state), $($w.type))") }
        [void]$sb.AppendLine("`n## Up for grabs this sprint")
        foreach ($w in $raw.ado.upForGrabs) { [void]$sb.AppendLine("- **#$($w.id)** $($w.title) ($($w.state), $($w.type))") }
        [void]$sb.AppendLine("`n## PRs waiting for my review")
        foreach ($p in $raw.ado.prsAwaitingMyReview) { [void]$sb.AppendLine("- **!$($p.id)** $($p.title) - $($p.repo), by $($p.author)") }
        [void]$sb.AppendLine("`n## Failed builds")
        foreach ($b in $raw.ado.failedBuilds) { [void]$sb.AppendLine("- **$($b.pipeline)** on $($b.branch) ($($b.reason))") }
    }
    if ($raw.email) {
        [void]$sb.AppendLine("`n## Unread email (pre-filtered only)")
        foreach ($m in $raw.email.messages) { [void]$sb.AppendLine("- **$($m.from)** - $($m.subject)") }
    }
    [void]$sb.AppendLine("`n## Worktrees")
    foreach ($w in $raw.worktrees) { [void]$sb.AppendLine("- $($w.path) -> $($w.branch)$(if ($w.isT3) { ' (T3)' })") }
    return $sb.ToString()
}

# When Claude's login has expired, the note opens with a callout and a double-clickable script that
# signs in (browser) and regenerates the report. The script is an ordinary note attachment.
function Write-LoginFix([string]$Folder) {
    $attachments = Join-Path $Folder 'attachments'
    $fix = Join-Path $attachments 'fix-claude-login.cmd'
    if (-not $script:ClaudeNeedsLogin) {
        if (Test-Path $fix) { Remove-Item $fix }
        return ''
    }
    New-Item -ItemType Directory -Force $attachments | Out-Null
    $cmd = @(
        '@echo off',
        'title Stickies: sign in to Claude',
        'echo Signing in to Claude Code (a browser window will open)...',
        'claude auth login',
        'if errorlevel 1 ( echo. & echo Sign-in did not complete. & pause & exit /b 1 )',
        'echo.',
        'echo Signed in. Regenerating the morning report...',
        ('powershell -NoProfile -ExecutionPolicy Bypass -File "{0}"' -f (Join-Path $PSScriptRoot 'Invoke-MorningReport.ps1')),
        'echo Done. The sticky note updates by itself.',
        'timeout /t 5'
    )
    [IO.File]::WriteAllLines($fix, $cmd, [Text.ASCIIEncoding]::new())
    return @(
        '> **Claude needs to sign in again.** Its login has expired, so this is the unfiltered listing.',
        '> Double-click to fix: [Sign in to Claude and regenerate this report](attachments/fix-claude-login.cmd)',
        '> To stop this recurring, run `claude setup-token` once and store the token in a user environment',
        '> variable named `CLAUDE_CODE_OAUTH_TOKEN` (see morning-report/README.md).',
        ''
    ) -join "`n"
}

function Write-Note([string]$DataDir, [string]$Body) {
    $today = Get-Date
    $folder = Join-Path $DataDir ("notes\{0:yyyy-MM-dd}-morning-report" -f $today)
    New-Item -ItemType Directory -Force $folder | Out-Null
    $header = ("# Morning report, {0:dddd d MMMM}`n`n" -f $today) + (Write-LoginFix $folder)
    $failures = if ($script:Failures) { "`n---`n_Problems: " + ($script:Failures -join '; ') + "_`n" } else { '' }
    $content = "---`ncolor: `"#CFE8FF`"`ncreated: {0}`n---`n`n{1}{2}{3}" -f $today.ToString('yyyy-MM-ddTHH:mm:sszzz'), $header, $Body.Trim(), $failures
    $tmp = Join-Path $folder 'note.md.tmp'
    [IO.File]::WriteAllText($tmp, $content, [Text.UTF8Encoding]::new($false))
    Move-Item -Force $tmp (Join-Path $folder 'note.md')
    return $folder
}

# The first thing the run does is put a placeholder note on screen, so "no report" is visibly
# different from "generator never started". Write-Note replaces it when the real report is ready.
function Write-Placeholder([string]$DataDir, [string]$Status) {
    $today = Get-Date
    $folder = Join-Path $DataDir ("notes\{0:yyyy-MM-dd}-morning-report" -f $today)
    New-Item -ItemType Directory -Force $folder | Out-Null
    $status = if ($Status) { " $Status" } else { '' }
    $content = "---`ncolor: `"#E8E8E8`"`ncreated: {0}`n---`n`n# Morning report, {1:dddd d MMMM}`n`n_Generating, started {1:HH:mm}.{2}_`n`nIf this text is still here after a few minutes the generator failed; see ``{3}``.`n" -f $today.ToString('yyyy-MM-ddTHH:mm:sszzz'), $today, $status, $logFile
    $tmp = Join-Path $folder 'note.md.tmp'
    [IO.File]::WriteAllText($tmp, $content, [Text.UTF8Encoding]::new($false))
    Move-Item -Force $tmp (Join-Path $folder 'note.md')
}

# ---------------------------------------------------------------- main
Write-Log 'Morning report starting'
$dataDir = Get-DataDir
Write-Log "Data dir: $dataDir"
Write-Placeholder $dataDir ''

# Task Scheduler's "restart on failure" only covers failure to launch, not a non-zero exit, so
# transient problems (no network yet after wake) are retried here instead.
$maxAttempts = 3
for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
    $script:Failures = @()
    $raw = [ordered]@{
        generatedAt = (Get-Date).ToString('o')
        worktrees = @(Get-Worktrees)
        ado = $null
        email = $null
    }
    try { $raw.ado = Get-AdoData -Worktrees $raw.worktrees } catch { Add-Failure 'Azure DevOps' $_.Exception.Message }
    try { $raw.email = Get-EmailData } catch { Add-Failure 'Email' $_.Exception.Message }
    $transient = @($script:Failures | Where-Object { $_ -match 'remote name could not be resolved|Unable to connect|timed out|\(50[0-9]\)|network' })
    if (-not $transient -or $attempt -eq $maxAttempts) { break }
    Write-Log "Attempt $attempt hit a transient failure; retrying in 3 minutes: $($transient -join '; ')"
    Write-Placeholder $dataDir "Attempt $attempt failed ($($transient -join '; ')); retrying at $((Get-Date).AddMinutes(3).ToString('HH:mm'))."
    Start-Sleep -Seconds 180
}

# -Compress: Windows PowerShell's pretty printer pads nested JSON with absurd whitespace, and the
# compact form costs Claude fewer tokens. Open last.raw.json in a formatter to read it.
$rawJson = $raw | ConvertTo-Json -Depth 8 -Compress
if ($KeepRaw) { $rawJson | Set-Content "$PSScriptRoot\last.raw.json" -Encoding utf8 }

$body = if ($NoClaude) { $null } else { Invoke-Claude $rawJson }
if (-not $body) { $body = Format-Fallback $raw }

$written = Write-Note $dataDir $body
Write-Log "Wrote $written"
if ($script:ClaudeNeedsLogin) {
    # A retry cannot fix an expired login; the note tells the user how to. Exit 0 so Task Scheduler
    # does not retry, unless something else also failed.
    Write-Log 'Claude login expired; the note offers a fix'
}
$retryable = @($script:Failures | Where-Object { $_ -notlike 'Claude: *login expired*' })
if ($retryable) {
    Write-Log "Finished with $($retryable.Count) retryable failure(s)"
    exit 1
}
Write-Log 'Finished'
