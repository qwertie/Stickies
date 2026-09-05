<#
.SYNOPSIS
  Registers (or updates) the Windows scheduled task that runs the morning report at 04:00,
  waking the PC if it is asleep and running as soon as possible if the time was missed.
#>
[CmdletBinding()]
param(
    [string]$At = '04:00',
    [string]$TaskName = 'Stickies Morning Report'
)

$script = Join-Path $PSScriptRoot 'Invoke-MorningReport.ps1'
$action = New-ScheduledTaskAction -Execute 'powershell.exe' `
    -Argument "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$script`""
$trigger = New-ScheduledTaskTrigger -Daily -At $At
$settings = New-ScheduledTaskSettingsSet -WakeToRun -StartWhenAvailable `
    -ExecutionTimeLimit (New-TimeSpan -Minutes 30) -MultipleInstances IgnoreNew `
    -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 10)

# "Run only when user is logged on" (Interactive) so the task can reach the user's Graph token
# cache, Claude login and PAT environment variable. Waking to the lock screen still counts.
$principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited

Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings `
    -Principal $principal -Force | Out-Null
Write-Host "Registered '$TaskName' daily at $At (wake to run, run if missed). Test it with:"
Write-Host "  Start-ScheduledTask -TaskName '$TaskName'"
