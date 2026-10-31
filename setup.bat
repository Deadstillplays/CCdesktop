@echo off
setlocal enabledelayedexpansion
title CCdesktop Setup

echo.
echo  ==============================================
echo    CCdesktop - Claude Desktop Remake (Rust)
echo  ==============================================
echo.

:: ── Check for Rust / cargo ──────────────────────────────────────────────────
where cargo >nul 2>&1
if %errorlevel% neq 0 (
    echo  [!] Rust not found. Downloading rustup installer...
    echo.
    powershell -NoProfile -Command ^
        "Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile '%TEMP%\rustup-init.exe'"
    if %errorlevel% neq 0 (
        echo  [ERROR] Failed to download rustup. Check your internet connection.
        pause & exit /b 1
    )
    echo  [*] Running rustup installer (follow the prompts, choose default install)...
    "%TEMP%\rustup-init.exe" -y
    if %errorlevel% neq 0 (
        echo  [ERROR] rustup installation failed.
        pause & exit /b 1
    )
    :: Reload PATH so cargo is visible this session
    set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
    echo.
    echo  [OK] Rust installed successfully.
) else (
    for /f "tokens=*" %%v in ('cargo --version 2^>nul') do echo  [OK] Found %%v
)

echo.
echo  [*] Building release binary (this takes ~1-3 min the first time)...
echo.

:: ── Build ───────────────────────────────────────────────────────────────────
cargo build --release
if %errorlevel% neq 0 (
    echo.
    echo  [ERROR] Build failed. See errors above.
    pause & exit /b 1
)

set "EXE=%~dp0target\release\claude-desktop-rust.exe"
if not exist "%EXE%" (
    echo  [ERROR] Expected binary not found at: %EXE%
    pause & exit /b 1
)

echo.
echo  [OK] Build complete!

:: ── Create Desktop shortcut ─────────────────────────────────────────────────
echo  [*] Creating desktop shortcut...
powershell -NoProfile -Command ^
    "$ws = New-Object -COM WScript.Shell; ^
     $sc = $ws.CreateShortcut([Environment]::GetFolderPath('Desktop') + '\CCdesktop.lnk'); ^
     $sc.TargetPath = '%EXE:\=\\%'; ^
     $sc.WorkingDirectory = '%~dp0target\release\'.TrimEnd('\'); ^
     $sc.Description = 'CCdesktop - Claude Desktop Remake'; ^
     $sc.Save()"

if %errorlevel% equ 0 (
    echo  [OK] Shortcut created on your Desktop.
) else (
    echo  [!] Could not create shortcut (non-fatal).
)

:: ── Copy run.bat next to the exe for convenience ────────────────────────────
copy /y "%~dp0run.bat" "%~dp0target\release\run.bat" >nul 2>&1

echo.
echo  ============================================================
echo    Done!
echo.
echo    Run the app:
echo      * Double-click CCdesktop on your Desktop
echo      * Or run: target\release\claude-desktop-rust.exe
echo.
echo    On first launch, go to Settings and add your
echo    Claude session key (or paste it via the account menu).
echo  ============================================================
echo.
pause
