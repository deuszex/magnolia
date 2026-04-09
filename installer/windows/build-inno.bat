@echo off
REM Build Windows installer using Inno Setup
REM Download Inno Setup from: https://jrsoftware.org/isinfo.php

setlocal enabledelayedexpansion

set SCRIPT_DIR=%~dp0
set PROJECT_ROOT=%SCRIPT_DIR%..\..
set VERSION=1.0.0

echo Building magnolia Windows Installer v%VERSION% (Inno Setup)
echo.

echo Building release binaries...
cd /d "%PROJECT_ROOT%\backend"
cargo build --release --bin magnolia_server --bin service_ctl --bin create_admin
if errorlevel 1 (
 echo Error: Cargo build failed
 exit /b 1
)

set BIN_DIR=%PROJECT_ROOT%\target\release

REM Check required binaries
if not exist "%BIN_DIR%\magnolia_server.exe" (
 echo Error: magnolia_server.exe not found at %BIN_DIR%
 exit /b 1
)
if not exist "%BIN_DIR%\service_ctl.exe" (
 echo Error: service_ctl.exe not found at %BIN_DIR%
 exit /b 1
)
if not exist "%BIN_DIR%\create_admin.exe" (
 echo Error: create_admin.exe not found at %BIN_DIR%
 exit /b 1
)

REM Find Inno Setup compiler
set ISCC=
if exist "%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe" (
 set "ISCC=%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe"
) else if exist "%ProgramFiles%\Inno Setup 6\ISCC.exe" (
 set "ISCC=%ProgramFiles%\Inno Setup 6\ISCC.exe"
) else if exist "%LocalAppData%\Programs\Inno Setup 6\ISCC.exe" (
 set "ISCC=%LocalAppData%\Programs\Inno Setup 6\ISCC.exe"
) else (
 echo Error: Inno Setup not found. Install from https://jrsoftware.org/isinfo.php
 exit /b 1
)

REM Build installer
echo.
echo Building installer with Inno Setup...
cd /d "%SCRIPT_DIR%"
"%ISCC%" /DMyAppVersion=%VERSION% magnolia.iss

if errorlevel 1 (
 echo Error: Inno Setup build failed
 exit /b 1
)

echo.
echo Build complete: %SCRIPT_DIR%magnolia-%VERSION%-Setup.exe
echo.
echo To install: Run magnolia-%VERSION%-Setup.exe (requires admin)
