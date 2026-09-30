@echo off
setlocal DisableDelayedExpansion
title Uninstall ADHD
if not exist "%~dp0scripts\uninstall-windows.ps1" (
    echo Please extract the entire ZIP first, then open Uninstall.cmd in the extracted folder.
    pause
    exit /b 1
)
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\uninstall-windows.ps1"
set "ADHD_UNINSTALL_EXIT=%ERRORLEVEL%"
echo.
pause
exit /b %ADHD_UNINSTALL_EXIT%
