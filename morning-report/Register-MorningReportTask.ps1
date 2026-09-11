<#
.SYNOPSIS
  Registers (or updates) the Windows scheduled task that runs the morning report at 04:00,
  waking the PC if it is asleep and running as soon as possible if the time was missed.

.DESCRIPTION
  The task does not run Invoke-MorningReport.ps1 directly. It runs a tiny launcher written to
  %LOCALAPPDATA%\Stickies (always on the system drive) which waits for the script's drive to be
  available, logs any launch problem to %LOCALAPPDATA%\Stickies\morning-report-launch.log, and
  then runs the script by its real path. Reason: if this folder is reached through a junction or
  lives on a USB disk, it may not exist yet in the seconds after the PC wakes, and a direct
  `powershell -File` would fail before anything could be logged.
#>
[CmdletBinding()]
param(
    [string]$At = '04:00',
    [string]$TaskName = 'Stickies Morning Report'
)

. (Join-Path $PSScriptRoot '..\scripts\Common.ps1')

$script = Join-Path (Resolve-RealPath $PSScriptRoot) 'Invoke-MorningReport.ps1'
if (-not (Test-Path $script)) { throw "Cannot find $script" }

$launcherDir = Join-Path $env:LOCALAPPDATA 'Stickies'
New-Item -ItemType Directory -Force $launcherDir | Out-Null
$launcher = Join-Path $launcherDir 'run-morning-report.ps1'
$launcherBody = @"
# Written by Register-MorningReportTask.ps1; runs the morning report once its drive is available.
`$script = '$script'
`$log = Join-Path `$PSScriptRoot 'morning-report-launch.log'
function Note(`$m) { Add-Content `$log ("{0:yyyy-MM-dd HH:mm:ss} {1}" -f (Get-Date), `$m) }
`$deadline = (Get-Date).AddMinutes(5)
while (-not (Test-Path `$script) -and (Get-Date) -lt `$deadline) { Start-Sleep -Seconds 10 }
if (-not (Test-Path `$script)) { Note "Gave up: `$script not found after 5 minutes (drive not mounted?)"; exit 2 }
Note "Launching `$script"
& `$script
Note "Finished with exit code `$LASTEXITCODE"
exit `$LASTEXITCODE
"@
[IO.File]::WriteAllText($launcher, $launcherBody, [Text.UTF8Encoding]::new($true))

$action = New-ScheduledTaskAction -Execute 'powershell.exe' `
    -Argument "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$launcher`""
$trigger = New-ScheduledTaskTrigger -Daily -At $At
$settings = New-ScheduledTaskSettingsSet -WakeToRun -StartWhenAvailable `
    -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit (New-TimeSpan -Minutes 45) -MultipleInstances IgnoreNew

# "Run only when user is logged on" (Interactive) so the task can reach the user's Graph token
# cache, Claude login and PAT environment variable. Waking to the lock screen still counts.
$principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited

Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings `
    -Principal $principal -Force | Out-Null
Write-Host "Registered '$TaskName' daily at $At (wake to run, run if missed)."
Write-Host "  launcher: $launcher"
Write-Host "  script:   $script"
Write-Host "Test it with:  Start-ScheduledTask -TaskName '$TaskName'"
