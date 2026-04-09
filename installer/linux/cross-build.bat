@echo off
:: Cross-compile Linux .deb package from Windows using cargo-zigbuild.
:: No Docker, no WSL required.
::
:: Prerequisites:
:: winget install zig.zig
:: cargo install cargo-zigbuild cargo-deb
::
:: Usage:
:: cross-build.bat
:: cross-build.bat --target aarch64-unknown-linux-gnu

setlocal

set "TARGET=x86_64-unknown-linux-gnu"
set "EXTRA="

:parse
if "%~1"=="" goto run
if /i "%~1"=="--target" (
 set "TARGET=%~2"
 shift & shift
 goto parse
)
set "EXTRA=%EXTRA% %~1"
shift
goto parse

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

where cargo-deb >nul 2>&1
if errorlevel 1 (
 echo cargo-deb not found. Installing...
 cargo install cargo-deb
)

set "BACKEND=%~dp0..\..\backend"
pushd "%BACKEND%"

rustup target add %TARGET% 2>nul

echo Building release binaries for %TARGET%...
cargo zigbuild --release --bin magnolia_server --bin service_ctl --bin create_admin --target %TARGET%
if errorlevel 1 goto fail

echo Building .deb package...
cargo deb --no-build --no-strip --target %TARGET%
if errorlevel 1 goto fail

popd
for /r "%~dp0..\..\target" %%f in (*.deb) do (
 copy "%%f" "%~dp0"
 echo.
 echo Build complete: %~dp0%%~nxf
 exit /b 0
)

:fail
echo Error: build failed.
popd
exit /b 1
