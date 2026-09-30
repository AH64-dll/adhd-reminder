param([string]$Binary = "$PSScriptRoot\..\target\release\adhd.exe", [switch]$NoStart)
$ErrorActionPreference = 'Stop'
if (!(Test-Path $Binary)) { throw 'Build first: cargo build --release --locked' }
$install = Join-Path $env:LOCALAPPDATA 'Programs\ADHD'
$config = Join-Path $env:APPDATA 'adhd'
New-Item -ItemType Directory -Force $install, $config | Out-Null
$exe = Join-Path $install 'ADHD.exe'
if (Test-Path $exe) {
    & $exe --quit
    Start-Sleep -Seconds 3
}
Copy-Item $Binary $exe -Force
$user = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
$taskName = 'ADHD Reminder ' + [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$action = New-ScheduledTaskAction -Execute $exe -Argument '--supervise'
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $user
$principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
Set-Content -Path (Join-Path $config 'installed') -Value $exe
Set-Content -Path (Join-Path $config 'task-name') -Value $taskName -Encoding Ascii
Remove-Item (Join-Path $config 'startup-disabled') -ErrorAction SilentlyContinue
$userPath = [string][Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $install) {
    [Environment]::SetEnvironmentVariable('Path', ($userPath.TrimEnd(';') + ';' + $install).TrimStart(';'), 'User')
}
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Programs')) 'ADHD.lnk'))
$shortcut.TargetPath = $exe
$shortcut.WorkingDirectory = $install
$shortcut.Description = 'A quiet reminder of your current goal'
$shortcut.Save()
if (!$NoStart) { Start-ScheduledTask -TaskName $taskName }
Add-Type -Namespace ADHD -Name EnvironmentRefresh -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
public static extern System.IntPtr SendMessageTimeout(System.IntPtr hwnd, uint msg, System.UIntPtr wParam, string lParam, uint flags, uint timeout, out System.UIntPtr result);
'@
$result = [UIntPtr]::Zero
[ADHD.EnvironmentRefresh]::SendMessageTimeout([IntPtr]0xffff, 0x1a, [UIntPtr]::Zero, 'Environment', 2, 2000, [ref]$result) | Out-Null
Write-Host 'Installed ADHD. Open a new terminal and type ADHD, or use the Start menu.'
