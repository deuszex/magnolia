@echo off
:: Cross-compile Linux .rpm package from Windows using cargo-zigbuild.
:: No Docker, no WSL required.
::
:: Prerequisites:
::   winget install zig.zig
::   cargo install cargo-zigbuild cargo-generate-rpm
::
:: Usage:
::   cross-build-rpm.bat
::   cross-build-rpm.bat --target aarch64-unknown-linux-gnu

setlocal

set "TARGET=x86_64-unknown-linux-gnu"

:parse
if "%~1"=="" goto run
if /i "%~1"=="--target" (
    set "TARGET=%~2"
    shift & shift
    goto parse
)
echo Unknown option: %~1
echo Usage: cross-build-rpm.bat [--target ^<rust-target-triple^>]
exit /b 1

:run
where zig >nul 2>&1
if errorlevel 1 (
    echo Error: zig not found.
    echo Install with: winget install zig.zig
    exit /b 1
)

where cargo-zigbuild >nul 2>&1
if errorlevel 1 (
    echo cargo-zigbuild not found. Installing...
    cargo install cargo-zigbuild
)

where cargo-generate-rpm >nul 2>&1
if errorlevel 1 (
    echo cargo-generate-rpm not found. Installing...
    cargo install cargo-generate-rpm
)

set "BACKEND=%~dp0..\..\..\backend"
pushd "%BACKEND%"

rustup target add %TARGET% 2>nul

echo Building release binaries for %TARGET%...
cargo zigbuild --release --bin magnolia_server --bin service_ctl --bin create_admin --target %TARGET%
if errorlevel 1 goto fail

echo Building .rpm package...
cargo generate-rpm --auto-req disabled --target %TARGET%
if errorlevel 1 goto fail

popd
for /r "%~dp0..\..\..\target" %%f in (*.rpm) do (
    copy "%%f" "%~dp0"
    echo.
    echo Build complete: %~dp0%%~nxf
    echo.
    echo To install:   sudo rpm -i %%~nxf
    echo               sudo dnf install ./%%~nxf
    echo To upgrade:   sudo rpm -U %%~nxf
    echo To uninstall: sudo rpm -e magnolia_server
    exit /b 0
)

echo Error: .rpm file not found.
exit /b 1

:fail
echo Error: build failed.
popd
exit /b 1
