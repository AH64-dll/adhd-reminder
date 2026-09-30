param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$UserName,
    [Parameter(Mandatory = $true)][string]$UserSid,
    [Parameter(Mandatory = $true)][string]$ResultFile
)
$ErrorActionPreference = 'Stop'
try {
    # Only task registration is elevated. The app and user-file installation remain
    # in the original user's process, including over-the-shoulder UAC approval.
    $sid = New-Object System.Security.Principal.SecurityIdentifier($UserSid)
    $account = New-Object System.Security.Principal.NTAccount($UserName)
    if ($account.Translate([System.Security.Principal.SecurityIdentifier]).Value -ne $sid.Value) {
        throw 'The startup account could not be verified.'
    }
    $taskName = 'ADHD Reminder ' + $sid.Value
    $action = New-ScheduledTaskAction -Execute $Executable -Argument '--supervise'
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $UserName
    $principal = New-ScheduledTaskPrincipal -UserId $sid.Value -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null

    # The original user must be able to toggle startup and uninstall even when
    # another administrator approved UAC. Keep SYSTEM and administrator access.
    $scheduler = New-Object -ComObject Schedule.Service
    $scheduler.Connect()
    $registered = $scheduler.GetFolder('\').GetTask($taskName)
    $registered.SetSecurityDescriptor(('D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;' + $sid.Value + ')'), 0)
    @{ ok = $true } | ConvertTo-Json | Set-Content -LiteralPath $ResultFile -Encoding UTF8
}
catch {
    @{ ok = $false; error = $_.Exception.Message } | ConvertTo-Json | Set-Content -LiteralPath $ResultFile -Encoding UTF8
    throw
}
