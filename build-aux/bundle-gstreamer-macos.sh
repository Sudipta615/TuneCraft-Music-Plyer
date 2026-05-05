#!/bin/bash
# ─────────────────────────────────────────────────────────────────────────────
# bundle-gstreamer-macos.sh — GStreamer bundling for macOS .app bundles
# ─────────────────────────────────────────────────────────────────────────────
#
# This script rewrites the dynamic library paths in the TuneCraft binary
# and the GStreamer libraries so they use @executable_path-relative paths
# inside the .app bundle. This ensures the application can find GStreamer
# at runtime without requiring the user to install GStreamer separately.
#
# Usage:
#   ./bundle-gstreamer-macos.sh <path-to-tunecraft-binary> <path-to-app-bundle>
#
# Prerequisites:
#   - GStreamer runtime installed (from gstreamer.freedesktop.org macOS .pkg)
#   - install_name_tool (macOS developer tools)
#   - macdylibbundler (optional, for recursive dependency scanning)
#
# ─────────────────────────────────────────────────────────────────────────────

set -euo pipefail

BINARY="${1:?Usage: $0 <binary> <app-bundle>}"
APP_BUNDLE="${2:?Usage: $0 <binary> <app-bundle>}"

FRAMEWORKS_DIR="${APP_BUNDLE}/Contents/Frameworks"
# Fix H15: Auto-detect GStreamer installation path. The script previously
# hardcoded the .pkg installer path, but CI uses Homebrew which installs
# to /opt/homebrew/ (ARM) or /usr/local/ (Intel).
if [ -d "/Library/Frameworks/GStreamer.framework/Versions/1.0" ]; then
    GST_PREFIX="/Library/Frameworks/GStreamer.framework/Versions/1.0"
elif [ -d "/opt/homebrew/Cellar/gstreamer" ]; then
    # Homebrew on Apple Silicon
    GST_PREFIX=$(brew --prefix gstreamer 2>/dev/null || echo "/opt/homebrew")
elif [ -d "/usr/local/Cellar/gstreamer" ]; then
    # Homebrew on Intel Mac
    GST_PREFIX=$(brew --prefix gstreamer 2>/dev/null || echo "/usr/local")
else
    echo "ERROR: GStreamer installation not found!"
    echo "Install from https://gstreamer.freedesktop.org/download/ or via Homebrew"
    exit 1
fi

echo "=== TuneCraft macOS GStreamer Bundler ==="
echo "Binary: ${BINARY}"
echo "App Bundle: ${APP_BUNDLE}"
echo ""

# ── Step 1: Create Frameworks directory ──────────────────────────────────────
mkdir -p "${FRAMEWORKS_DIR}/gst-runtime"

# ── Step 2: Copy GStreamer libraries ────────────────────────────────────────
echo "Copying GStreamer libraries..."

# Core GStreamer libraries
for lib in \
    libgstreamer-1.0.0.dylib \
    libgstbase-1.0.0.dylib \
    libgstaudio-1.0.0.dylib \
    libgstpbutils-1.0.0.dylib \
    libgsttag-1.0.0.dylib \
    libgstapp-1.0.0.dylib \
    libgstriff-1.0.0.dylib \
    libgstrtp-1.0.0.dylib \
    libgstsdp-1.0.0.dylib \
    libgstnet-1.0.0.dylib \
    libgstcontroller-1.0.0.dylib; do
    if [ -f "${GST_PREFIX}/lib/${lib}" ]; then
        cp -L "${GST_PREFIX}/lib/${lib}" "${FRAMEWORKS_DIR}/gst-runtime/"
        echo "  Copied: ${lib}"
    else
        echo "  WARNING: ${lib} not found at ${GST_PREFIX}/lib/"
    fi
done

# GLib dependencies (required by GStreamer)
for lib in \
    libglib-2.0.0.dylib \
    libgobject-2.0.0.dylib \
    libgio-2.0.0.dylib \
    libgmodule-2.0.0.dylib \
    libintl.8.dylib \
    libffi.8.dylib \
    # Fix L7: Removed libgthread-2.0.0.dylib (hasn't existed since GLib 2.32, 2012)
    # Fix L8: Changed libffi.7.dylib to libffi.8.dylib (recent GStreamer)
    libpcre2-8.0.dylib; do
    if [ -f "${GST_PREFIX}/lib/${lib}" ]; then
        cp -L "${GST_PREFIX}/lib/${lib}" "${FRAMEWORKS_DIR}/gst-runtime/"
        echo "  Copied: ${lib}"
    fi
done

# ── Step 3: Copy GStreamer plugins ──────────────────────────────────────────
echo "Copying GStreamer plugins..."
mkdir -p "${FRAMEWORKS_DIR}/gst-runtime/plugins"

# Essential plugins for music playback
PLUGINS=(
    # Playback / demuxing
    gstplayback.so
    gstaudioconvert.so
    gstaudioresample.so
    gstaudiotestsrc.so
    gstvolume.so
    gsttypefindfunctions.so
    gstdecodebin.so
    gstdecodebin2.so
    gsturidecodebin.so

    # Format-specific demuxers
    gstflac.so
    gstlame.so
    gstmpg123audiodec.so
    gstogg.so
    gstvorbis.so
    gstopus.so
    gstaac.so
    gstisomp4.so
    gstwavparse.so
    gstwavenc.so
    gstapetag.so
    gstid3demux.so
    gstid3tag.so

    # Audiomixer for crossfade
    gstaudiomixer.so

    # Spectrum / analysis
    gstspectrum.so

    # File source
    gstgio.so
    gsttcp.so
)

for plugin in "${PLUGINS[@]}"; do
    if [ -f "${GST_PREFIX}/lib/gstreamer-1.0/${plugin}" ]; then
        cp -L "${GST_PREFIX}/lib/gstreamer-1.0/${plugin}" "${FRAMEWORKS_DIR}/gst-runtime/plugins/"
        echo "  Copied plugin: ${plugin}"
    else
        echo "  WARNING: Plugin ${plugin} not found"
    fi
done

# ── Step 4: Rewrite library paths with install_name_tool ────────────────────
# Fix H13: install_name_tool cross-reference rewriting was broken because
# after changing -id on a library, subsequent iterations couldn't find
# the original reference (since the id already changed). The fix is to
# first collect all old IDs, then change all -ids, then rewrite all
# cross-references.
echo "Rewriting dynamic library paths..."

# Phase 1: Collect all current install names before any changes
declare -A OLD_IDS
echo "Phase 1: Collecting current install names..."
for lib in "${FRAMEWORKS_DIR}"/gst-runtime/*.dylib; do
    libname=$(basename "${lib}")
    current_id=$(otool -D "${lib}" | tail -1)
    if [ -n "${current_id}" ]; then
        OLD_IDS["${libname}"]="${current_id}"
        echo "  ${libname}: ${current_id}"
    fi
done

# Phase 2: Change all install names (ids) in one pass
echo "Phase 2: Changing install names..."
for lib in "${FRAMEWORKS_DIR}"/gst-runtime/*.dylib; do
    libname=$(basename "${lib}")
    install_name_tool -id "@executable_path/../Frameworks/gst-runtime/${libname}" "${lib}"
done

# Phase 3: Rewrite cross-references using the collected old IDs
echo "Phase 3: Rewriting cross-references..."
for lib in "${FRAMEWORKS_DIR}"/gst-runtime/*.dylib; do
    libname=$(basename "${lib}")
    for other_name in "${!OLD_IDS[@]}"; do
        old_id="${OLD_IDS[${other_name}]}"
        new_id="@executable_path/../Frameworks/gst-runtime/${other_name}"
        install_name_tool -change "${old_id}" "${new_id}" "${lib}" 2>/dev/null || true
    done
    echo "  Rewrote: ${libname}"
done

# Rewrite GStreamer references in the TuneCraft binary itself
for libname in "${!OLD_IDS[@]}"; do
    old_id="${OLD_IDS[${libname}]}"
    new_id="@executable_path/../Frameworks/gst-runtime/${libname}"
    install_name_tool -change "${old_id}" "${new_id}" "${BINARY}" 2>/dev/null || true
done

# Phase 3b: Rewrite cross-references in GStreamer plugins
# Fix: Plugin .so files were not having their cross-references rewritten,
# causing them to look for dependencies at the absolute /Library/Frameworks/... path.
echo "Phase 3b: Rewriting plugin cross-references..."
for plugin in "${FRAMEWORKS_DIR}"/gst-runtime/plugins/*.so; do
    if [ -f "${plugin}" ]; then
        plugin_name=$(basename "${plugin}")
        for other_name in "${!OLD_IDS[@]}"; do
            old_id="${OLD_IDS[${other_name}]}"
            new_id="@executable_path/../Frameworks/gst-runtime/${other_name}"
            install_name_tool -change "${old_id}" "${new_id}" "${plugin}" 2>/dev/null || true
        done
        echo "  Rewrote plugin: ${plugin_name}"
    fi
done

# ── Step 5: Set GST_PLUGIN_PATH in the launcher ─────────────────────────────
echo "Creating launcher script..."
mkdir -p "${APP_BUNDLE}/Contents/MacOS"

# Fix H14: Bundle gst-plugin-scanner so GStreamer can discover plugins.
if [ -f "${GST_PREFIX}/libexec/gstreamer-1.0/gst-plugin-scanner" ]; then
    cp -L "${GST_PREFIX}/libexec/gstreamer-1.0/gst-plugin-scanner" "${FRAMEWORKS_DIR}/gst-runtime/"
    chmod +x "${FRAMEWORKS_DIR}/gst-runtime/gst-plugin-scanner"
    echo "  Copied: gst-plugin-scanner"
else
    echo "  WARNING: gst-plugin-scanner not found"
fi

cat > "${APP_BUNDLE}/Contents/MacOS/Tunecraft-launcher" << LAUNCHER
#!/bin/bash
# TuneCraft macOS launcher — sets up GStreamer plugin path
DIR="$(cd "$(dirname "$0")" && pwd)"
export GST_PLUGIN_PATH="${DIR}/../Frameworks/gst-runtime/plugins"
export GST_PLUGIN_SYSTEM_PATH="${DIR}/../Frameworks/gst-runtime/plugins"
# Point GStreamer to the bundled plugin scanner
export GST_PLUGIN_SCANNER="${DIR}/../Frameworks/gst-runtime/gst-plugin-scanner"
# Disable GStreamer registry fork (not needed for bundled plugins)
export GST_REGISTRY_FORK="no"
exec "${DIR}/Tunecraft" "$@"
LAUNCHER

chmod +x "${APP_BUNDLE}/Contents/MacOS/Tunecraft-launcher"

echo ""
echo "=== Bundle complete ==="
echo "GStreamer libraries: $(ls "${FRAMEWORKS_DIR}/gst-runtime/"*.dylib 2>/dev/null | wc -l | tr -d ' ') dylibs"
echo "GStreamer plugins:   $(ls "${FRAMEWORKS_DIR}/gst-runtime/plugins/"*.so 2>/dev/null | wc -l | tr -d ' ') plugins"
echo ""
echo "NOTE: Run this script on macOS with GStreamer installed."
echo "The .app bundle can then be packaged into a .dmg using create-dmg."
