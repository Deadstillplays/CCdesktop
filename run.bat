@echo off
setlocal
title CCdesktop

set "EXE=%~dp0target\release\claude-desktop-rust.exe"

if not exist "%EXE%" (
    echo.
    echo  [!] Executable not found at: %EXE%
    echo  [*] Running setup to build the app first...
    echo.
    call "%~dp0setup.bat"
    exit /b %errorlevel%
)

start "" "%EXE%"
