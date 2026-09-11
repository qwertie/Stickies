<#
.SYNOPSIS
  Builds the Windows installer, replaces the running Stickies with it, and verifies it came back.

.DESCRIPTION
  One command for "make my running copy match the source":
    1. npm run build:win from the repo's real path (Vite rejects a junctioned project root)
    2. stop the running stickies process (notes are already on disk; saves happen within 1 s)
    3. run the freshly built NSIS installer silently, which upgrades %LOCALAPPDATA%\Stickies in
       place, refreshes the Start menu shortcut and autostart entry, and relaunches the app
    4. confirm the new process is running and report how many note windows it opened

  Exit code 0 only if every step succeeded. Use -SkipBuild to reinstall the last build.
#>
[CmdletBinding()]
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'scripts\Common.ps1')

$repo = Resolve-RealPath $PSScriptRoot
$app = Join-Path $repo 'app'
$installer = Join-Path $app 'src-tauri\target\release\bundle\nsis'

function Step([string]$Message) { Write-Host ("== {0}" -f $Message) -ForegroundColor Cyan }

function Get-DataDirForReport {
    $cfg = Join-Path $env:APPDATA 'Stickies\config.json'
    $dir = if (Test-Path $cfg) { (Get-Content $cfg -Raw | ConvertFrom-Json).dataDir } else { $null }
    if ($dir) { return $dir }
    if ($env:OneDrive -and (Test-Path $env:OneDrive)) { return Join-Path $env:OneDrive 'Stickies' }
    return Join-Path $env:USERPROFILE 'Stickies'
}

if (-not $SkipBuild) {
    Step "Building from $app"
    # Via cmd.exe into a log: under Windows PowerShell, 2>&1 on a native command turns each stderr
    # line (Tauri's progress output) into a terminating error when ErrorActionPreference is Stop.
    $log = Join-Path $env:TEMP 'stickies-deploy-build.log'
    Push-Location $app
    try {
        & cmd.exe /d /c "npm run build:win > `"$log`" 2>&1"
        $exit = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    $lines = Get-Content $log
    $lines | Where-Object { $_ -match '^(error|warning)(\[|:)|^\s+-->|Finished|Built application|Bundling' } | ForEach-Object { Write-Host $_ }
    if ($exit -ne 0) {
        $lines | Select-Object -Last 30 | ForEach-Object { Write-Host $_ }
        throw "npm run build:win failed with exit code $exit (full log: $log)"
    }
}

$setup = Get-ChildItem $installer -Filter 'Stickies_*-setup.exe' -ErrorAction Stop | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $setup) { throw "No installer found under $installer" }
Step "Installer: $($setup.Name) ($([math]::Round($setup.Length / 1MB, 1)) MB, built $($setup.LastWriteTime.ToString('HH:mm:ss')))"

Step 'Stopping the running Stickies'
Get-Process stickies -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 800

Step 'Installing silently'
$proc = Start-Process $setup.FullName -ArgumentList '/S' -Wait -PassThru
if ($proc.ExitCode -ne 0) { throw "Installer exited with code $($proc.ExitCode)" }

Step 'Waiting for Stickies to start'
$exe = Join-Path $env:LOCALAPPDATA 'Stickies\stickies.exe'
$deadline = (Get-Date).AddSeconds(10)
while (-not (Get-Process stickies -ErrorAction SilentlyContinue) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300 }
if (-not (Get-Process stickies -ErrorAction SilentlyContinue)) {
    Write-Host 'Installer did not relaunch the app; starting it'
    Start-Process $exe
    Start-Sleep -Seconds 2
}
Start-Sleep -Seconds 3   # let it open its windows
$running = Get-Process stickies -ErrorAction SilentlyContinue
if (-not $running) { throw 'Stickies is not running after install' }

$installed = (Get-Item $exe).LastWriteTime
$notes = (Get-ChildItem (Join-Path (Get-DataDirForReport) 'notes') -Directory -ErrorAction SilentlyContinue | Measure-Object).Count
Write-Host ("Deployed. stickies.exe written {0:HH:mm:ss}, pid {1}, {2} note folder(s) in the data dir." -f $installed, $running.Id, $notes) -ForegroundColor Green
