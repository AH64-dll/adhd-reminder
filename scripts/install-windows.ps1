param([string]$Binary, [switch]$NoStart, [switch]$RequestElevation)
$ErrorActionPreference = 'Stop'
try {
    if (![Environment]::Is64BitOperatingSystem) { throw 'This release requires 64-bit Windows.' }
    if (!$Binary) {
        $Binary = Join-Path $PSScriptRoot '..\bin\adhd.exe'
        if (!(Test-Path -LiteralPath $Binary -PathType Leaf)) {
            $Binary = Join-Path $PSScriptRoot '..\target\release\adhd.exe'
        }
    }
    if (!(Test-Path -LiteralPath $Binary -PathType Leaf)) {
        throw 'The ADHD app is missing. Extract the entire Windows ZIP before opening Setup.cmd. From source, build with cargo build --release --locked.'
    }
    $Binary = (Resolve-Path -LiteralPath $Binary).ProviderPath
    $install = Join-Path $env:LOCALAPPDATA 'Programs\ADHD'
    $config = Join-Path $env:APPDATA 'adhd'
    $exe = Join-Path $install 'ADHD.exe'
    $identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
    $user = $identity.Name
    $userSid = $identity.User.Value
    $taskName = 'ADHD Reminder ' + $userSid
    $helper = Join-Path $PSScriptRoot 'register-windows-startup.ps1'
    if (!(Test-Path -LiteralPath $helper -PathType Leaf)) { throw 'Setup files are missing. Extract the entire ZIP first.' }
    $resultFile = [IO.Path]::GetTempFileName()
    try {
        $principal = New-Object System.Security.Principal.WindowsPrincipal($identity)
        if ($RequestElevation -and !$principal.IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)) {
            Write-Host 'Approve the Windows permission prompt to enable startup when you log in.'
            # A UTF-16 encoded command avoids shell interpretation of spaces, Unicode,
            # apostrophes, ampersands, percent signs, or exclamation marks in file paths.
            function Quote-Literal([string]$Value) { return "'" + $Value.Replace("'", "''") + "'" }
            $invocation = '& ' + (Quote-Literal $helper) + ' -Executable ' + (Quote-Literal $exe) + ' -UserName ' + (Quote-Literal $user) + ' -UserSid ' + (Quote-Literal $userSid) + ' -ResultFile ' + (Quote-Literal $resultFile)
            $command = 'try { ' + $invocation + '; exit 0 } catch { exit 1 }'
            $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
            try {
                $elevated = Start-Process -FilePath (Join-Path $PSHOME 'powershell.exe') -Verb RunAs -ArgumentList ('-NoLogo -NoProfile -ExecutionPolicy Bypass -EncodedCommand ' + $encoded) -Wait -PassThru
            }
            catch {
                throw 'The administrator permission request was canceled or could not open. Run Setup.cmd again and approve the Windows prompt.'
            }
            if ($elevated.ExitCode -ne 0 -and !(Get-Content -LiteralPath $resultFile -Raw)) {
                throw 'Windows could not register login startup. Run Setup.cmd again or contact your administrator.'
            }
        }
        else {
            & $helper -Executable $exe -UserName $user -UserSid $userSid -ResultFile $resultFile
        }
        $result = Get-Content -LiteralPath $resultFile -Raw | ConvertFrom-Json
        if (!$result.ok) { throw ('Windows could not register login startup: ' + $result.error) }
    }
    finally {
        Remove-Item -LiteralPath $resultFile -Force -ErrorAction SilentlyContinue
    }
    New-Item -ItemType Directory -Force $install, $config | Out-Null
    if (Test-Path $exe) {
        # A stopped instance is normal during an upgrade; its exit code is harmless.
        Start-Process -FilePath $exe -ArgumentList '--quit' -Wait | Out-Null
        Start-Sleep -Seconds 3
    }
    Copy-Item -LiteralPath $Binary -Destination $exe -Force
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
    Write-Host ''
    Write-Host 'ADHD is installed and will start when you log in.'
    Write-Host 'Open ADHD from the Start menu, or open a new terminal and type ADHD.'
    exit 0
}
catch {
    Write-Host ''
    Write-Host ('Setup did not finish: ' + $_.Exception.Message) -ForegroundColor Red
    Write-Host 'Your saved goals are kept. Fix the problem above and run Setup.cmd again.'
    exit 1
}
