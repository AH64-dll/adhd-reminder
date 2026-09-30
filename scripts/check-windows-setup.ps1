param([string]$Binary = "$PSScriptRoot\..\target\release\adhd.exe", [switch]$ParseOnly)
$ErrorActionPreference = 'Stop'
foreach ($script in Get-ChildItem -LiteralPath $PSScriptRoot -Filter '*.ps1') {
    $tokens = $null
    $errors = $null
    [System.Management.Automation.Language.Parser]::ParseFile($script.FullName, [ref]$tokens, [ref]$errors) | Out-Null
    if ($errors.Count) { throw ($script.Name + ': ' + ($errors.Message -join '; ')) }
}
Write-Host 'Windows setup scripts parsed successfully.'
if ($ParseOnly) { exit 0 }

function Assert([bool]$Condition, [string]$Message) {
    if (!$Condition) { throw $Message }
}
function Invoke-Script([string]$Script, [string[]]$Arguments = @()) {
    & "$PSHOME\powershell.exe" -NoLogo -NoProfile -ExecutionPolicy Bypass -File $Script @Arguments
    if ($LASTEXITCODE -ne 0) { throw ($Script + ' failed with exit code ' + $LASTEXITCODE) }
}

$originalLocal = $env:LOCALAPPDATA
$originalRoaming = $env:APPDATA
$originalPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('ADHD setup ' + [guid]::NewGuid().ToString())
# An actual installer run exercises native argument handling and literal paths.
$bundle = Join-Path $temporary ("ADHD's setup & 100% ready! " + [char]0x03A9)
$taskName = 'ADHD Reminder ' + [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'ADHD.lnk'
$installed = $false
try {
    New-Item -ItemType Directory -Force (Join-Path $bundle 'scripts'), (Join-Path $bundle 'bin') | Out-Null
    Copy-Item -Path "$PSScriptRoot\*.ps1" -Destination (Join-Path $bundle 'scripts')
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $bundle 'bin\adhd.exe')
    $env:LOCALAPPDATA = Join-Path $temporary 'Local'
    $env:APPDATA = Join-Path $temporary 'Roaming'
    $install = Join-Path $env:LOCALAPPDATA 'Programs\ADHD'
    $data = Join-Path $env:LOCALAPPDATA 'adhd\adhd\data'
    New-Item -ItemType Directory -Force $data | Out-Null
    $saved = Join-Path $data 'state.json'
    [IO.File]::WriteAllText($saved, 'Preserve this saved goal data')
    $installer = Join-Path $bundle 'scripts\install-windows.ps1'
    $uninstaller = Join-Path $bundle 'scripts\uninstall-windows.ps1'
    # CI is already elevated. UAC consent itself requires a real interactive desktop.
    Invoke-Script $installer @('-NoStart', '-RequestElevation')
    $installed = $true
    $exe = Join-Path $install 'ADHD.exe'
    Assert (Test-Path -LiteralPath $exe) 'Installed executable is missing.'
    Assert (Test-Path -LiteralPath $shortcut) 'Start menu shortcut is missing.'
    Assert ([IO.File]::ReadAllText($saved) -eq 'Preserve this saved goal data') 'Setup changed goal data.'
    $task = Get-ScheduledTask -TaskName $taskName
    Assert ($task.Principal.RunLevel -eq 'Limited') 'The reminder must run without elevation.'
    Assert ($task.Principal.LogonType -eq 'Interactive') 'The reminder must use an interactive login.'
    Assert ($task.Actions.Execute -eq $exe) 'The login task points to the wrong executable.'
    $scheduler = New-Object -ComObject Schedule.Service
    $scheduler.Connect()
    $permissions = $scheduler.GetFolder('\').GetTask($taskName).GetSecurityDescriptor(4)
    $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    Assert ($permissions.Contains('(A;;GA;;;' + $sid + ')')) 'The original user cannot manage startup.'
    & schtasks.exe /Change /TN $taskName /DISABLE | Out-Null
    Assert ($LASTEXITCODE -eq 0) 'Disabling startup failed.'
    & schtasks.exe /Change /TN $taskName /ENABLE | Out-Null
    Assert ($LASTEXITCODE -eq 0) 'Enabling startup failed.'
    Invoke-Script $installer @('-NoStart', '-RequestElevation')
    Assert ([IO.File]::ReadAllText($saved) -eq 'Preserve this saved goal data') 'Upgrade changed goal data.'
    Invoke-Script $uninstaller
    $installed = $false
    Assert (!(Test-Path -LiteralPath $exe)) 'Uninstall left the executable.'
    Assert (!(Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue)) 'Uninstall left login startup.'
    Assert (!(Test-Path -LiteralPath $shortcut)) 'Uninstall left the Start menu shortcut.'
    Assert ([IO.File]::ReadAllText($saved) -eq 'Preserve this saved goal data') 'Uninstall deleted goal data.'
    Write-Host 'Windows install, upgrade, startup permissions/toggles, and uninstall passed.'
}
finally {
    if ($installed) { Invoke-Script $uninstaller }
    [Environment]::SetEnvironmentVariable('Path', $originalPath, 'User')
    $env:LOCALAPPDATA = $originalLocal
    $env:APPDATA = $originalRoaming
    Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
}
