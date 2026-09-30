@echo off
setlocal DisableDelayedExpansion
title ADHD Setup
echo ADHD Setup
echo.
if not exist "%~dp0scripts\install-windows.ps1" (
    echo Please extract the entire ZIP first, then open Setup.cmd in the extracted folder.
    pause
    exit /b 1
)
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\install-windows.ps1" -RequestElevation
set "ADHD_SETUP_EXIT=%ERRORLEVEL%"
echo.
pause
exit /b %ADHD_SETUP_EXIT%
