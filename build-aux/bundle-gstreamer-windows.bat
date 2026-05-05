@echo off
REM ────────────────────────────────────────────────────────────────────────────
REM bundle-gstreamer-windows.bat — GStreamer DLL bundling for Windows
REM ────────────────────────────────────────────────────────────────────────────
REM
REM This script copies the required GStreamer DLLs to the application's
REM runtime directory and creates a launcher that prepends the DLL path
REM to the system PATH. This ensures the application can find GStreamer
REM at runtime without requiring the user to install GStreamer separately
REM or modify system environment variables.
REM
REM Usage:
REM   bundle-gstreamer-windows.bat <path-to-tunecraft-exe> <gst-runtime-dir>
REM
REM Prerequisites:
REM   - GStreamer MSVC 64-bit runtime (from gstreamer.freedesktop.org)
REM     Download the MSVC installer, install, and point <gst-runtime-dir>
REM     to the installed directory (e.g., C:\gstreamer\1.0\msvc_x86_64)
REM   - The TuneCraft binary must be compiled with MSVC toolchain
REM
REM ────────────────────────────────────────────────────────────────────────────

setlocal enabledelayedexpansion

set "BINARY=%~1"
set "GST_RUNTIME=%~2"

if "%BINARY%"=="" (
    echo Usage: %~nx0 ^<path-to-tunecraft.exe^> ^<gst-runtime-dir^>
    echo Example: %~nx0 target\release\tunecraft.exe C:\gstreamer\1.0\msvc_x86_64
    exit /b 1
)

if "%GST_RUNTIME%"=="" (
    echo Usage: %~nx0 ^<path-to-tunecraft.exe^> ^<gst-runtime-dir^>
    exit /b 1
)

echo === TuneCraft Windows GStreamer Bundler ===
echo Binary: %BINARY%
echo GStreamer Runtime: %GST_RUNTIME%
echo.

REM ── Step 1: Create gst-runtime subdirectory ─────────────────────────────────
set "APP_DIR=%~dp1"
set "GST_BUNDLE=%APP_DIR%gst-runtime"

echo Creating gst-runtime directory: %GST_BUNDLE%
mkdir "%GST_BUNDLE%" 2>nul

REM ── Step 2: Copy core GStreamer DLLs ────────────────────────────────────────
echo Copying GStreamer DLLs...

set "GST_BIN=%GST_RUNTIME%\bin"

for %%D in (
    gstreamer-1.0-0.dll
    gstbase-1.0-0.dll
    gstaudio-1.0-0.dll
    gstpbutils-1.0-0.dll
    gsttag-1.0-0.dll
    gstapp-1.0-0.dll
    gstriff-1.0-0.dll
    gstrtp-1.0-0.dll
    gstsdp-1.0-0.dll
    gstnet-1.0-0.dll
    gstcontroller-1.0-0.dll
    glib-2.0-0.dll
    gobject-2.0-0.dll
    gio-2.0-0.dll
    gmodule-2.0-0.dll
    intl-8.dll
    ffi-7.dll
    pcre2-8-0.dll
    zlib1.dll
    libwinpthread-1.dll
) do (
    if exist "%GST_BIN%\%%D" (
        copy /Y "%GST_BIN%\%%D" "%GST_BUNDLE%\" >nul
        echo   Copied: %%D
    ) else (
        echo   WARNING: %%D not found in %GST_BIN%
    )
)

REM ── Step 3: Copy GStreamer plugins ──────────────────────────────────────────
echo Copying GStreamer plugins...

set "GST_PLUGINS_DIR=%GST_RUNTIME%\lib\gstreamer-1.0"
set "GST_BUNDLE_PLUGINS=%GST_BUNDLE%\plugins"
mkdir "%GST_BUNDLE_PLUGINS%" 2>nul

for %%P in (
    gstplayback.dll
    gstaudioconvert.dll
    gstaudioresample.dll
    gstvolume.dll
    gsttypefindfunctions.dll
    gstdecodebin.dll
    gsturidecodebin.dll
    gstflac.dll
    gstlame.dll
    gstmpg123.dll
    gstogg.dll
    gstvorbis.dll
    gstopus.dll
    gstaac.dll
    gstisomp4.dll
    gstwavparse.dll
    gstapetag.dll
    gstid3demux.dll
    gstaudiomixer.dll
    gstspectrum.dll
    gstgio.dll
) do (
    if exist "%GST_PLUGINS_DIR%\%%P" (
        copy /Y "%GST_PLUGINS_DIR%\%%P" "%GST_BUNDLE_PLUGINS%\" >nul
        echo   Copied plugin: %%P
    ) else (
        echo   WARNING: Plugin %%P not found
    )
)

REM ── Step 4: Create launcher script ──────────────────────────────────────────
echo Creating launcher script...

set "LAUNCHER=%APP_DIR%Tunecraft-launcher.bat"

(
echo @echo off
echo REM TuneCraft Windows launcher - sets up GStreamer PATH
echo set "GST_RUNTIME_DIR=%~dp0gst-runtime"
echo set "PATH=%~dp0gst-runtime;%PATH%"
echo set "GST_PLUGIN_PATH=%~dp0gst-runtime\plugins"
echo set "GST_PLUGIN_SYSTEM_PATH=%~dp0gst-runtime\plugins"
echo REM Disable registry fork for bundled plugins
echo set "GST_REGISTRY_FORK=no"
echo start "" "%~dp0Tunecraft.exe" %*
) > "%LAUNCHER%"

echo.
echo === Bundle complete ===
echo.
echo To distribute: Include Tunecraft.exe, gst-runtime/, and Tunecraft-launcher.bat
echo Users should run Tunecraft-launcher.bat instead of Tunecraft.exe directly.
echo.
echo For MSI packaging, the launcher script will be replaced by a proper
echo PATH setup in the cargo-wix configuration.
