$ErrorActionPreference = 'Stop'
$install = Join-Path $env:LOCALAPPDATA 'Programs\ADHD'
$exe = Join-Path $install 'ADHD.exe'
if (Test-Path $exe) { & $exe --quit; Start-Sleep -Seconds 3 }
$taskName = 'ADHD Reminder ' + [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
$path = [Environment]::GetEnvironmentVariable('Path', 'User')
[Environment]::SetEnvironmentVariable('Path', (($path -split ';' | Where-Object { $_ -ne $install }) -join ';'), 'User')
Remove-Item (Join-Path ([Environment]::GetFolderPath('Programs')) 'ADHD.lnk') -ErrorAction SilentlyContinue
Remove-Item (Join-Path $env:APPDATA 'adhd') -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item $install -Recurse -Force -ErrorAction SilentlyContinue
Write-Host 'ADHD uninstalled. Saved goals/settings remain in your local app data.'
