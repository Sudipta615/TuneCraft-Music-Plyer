#!/bin/bash
# ─────────────────────────────────────────────────────────────────────────────
# verify-gstreamer-linux.sh — Runtime GStreamer version check for Linux
# ─────────────────────────────────────────────────────────────────────────────
#
# On Linux, GStreamer is provided by the distribution's package manager.
# This script verifies that the required GStreamer version and plugins
# are available at runtime, and prints a diagnostic report.
#
# TuneCraft requires GStreamer >= 1.20 for reliable crossfade/gapless.
#
# Usage:
#   ./verify-gstreamer-linux.sh
#
# Exit codes:
#   0 — All checks passed
#   1 — Missing GStreamer or insufficient version
#
# ─────────────────────────────────────────────────────────────────────────────

set -euo pipefail

MINIMUM_VERSION="1.20"
ERRORS=0

echo "=== TuneCraft Linux GStreamer Verification ==="
echo ""

# ── Check GStreamer installation ──────────────────────────────────────────────
if ! command -v gst-inspect-1.0 &>/dev/null; then
    echo "ERROR: gst-inspect-1.0 not found. GStreamer is not installed."
    echo "  Install with: sudo apt install libgstreamer1.0-dev gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav"
    echo "  Or on Fedora: sudo dnf install gstreamer1-devel gstreamer1-plugins-base-devel gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-libav"
    exit 1
fi

# ── Check GStreamer version ───────────────────────────────────────────────────
GST_VERSION=$(gst-inspect-1.0 --version 2>/dev/null | head -1 | grep -oP '\d+\.\d+' | head -1)
echo "GStreamer version: ${GST_VERSION:-unknown}"

if [ -n "${GST_VERSION}" ]; then
    # Compare versions (simple major.minor comparison)
    GST_MAJOR=$(echo "$GST_VERSION" | cut -d. -f1)
    GST_MINOR=$(echo "$GST_VERSION" | cut -d. -f2)
    MIN_MAJOR=$(echo "$MINIMUM_VERSION" | cut -d. -f1)
    MIN_MINOR=$(echo "$MINIMUM_VERSION" | cut -d. -f2)
    
    if [ "$GST_MAJOR" -lt "$MIN_MAJOR" ] || { [ "$GST_MAJOR" -eq "$MIN_MAJOR" ] && [ "$GST_MINOR" -lt "$MIN_MINOR" ]; }; then
        echo "ERROR: GStreamer version ${GST_VERSION} is below minimum ${MINIMUM_VERSION}"
        ERRORS=$((ERRORS + 1))
    else
        echo "  OK: Version meets minimum requirement (${MINIMUM_VERSION}+)"
    fi
fi

echo ""

# ── Check required plugins ────────────────────────────────────────────────────
echo "Checking required plugins..."

REQUIRED_PLUGINS=(
    # Core / playback
    "playback:GstPlayBin"
    "audioconvert:GstAudioConvert"
    "audioresample:GstAudioResample"
    "volume:GstVolume"
    "uridecodebin:GstURIDecodeBin"
    "decodebin:GstDecodeBin"
    
    # Format support
    "flac:GstFlacDec"
    "lame:GstLameEnc"
    "mpg123:GstMpg123AudioDec"
    "ogg:GstOggDemux"
    "vorbis:GstVorbisDec"
    "opus:GstOpusDec"
    "isomp4:GstQTDemux"
    "wavparse:GstWavParse"
    
    # Crossfade support
    "audiomixer:GstAudioMixer"
    
    # Tag reading
    "id3demux:GstID3Demux"
    "apetag:GstApeTagDemux"
)

for plugin_spec in "${REQUIRED_PLUGINS[@]}"; do
    plugin_name="${plugin_spec%%:*}"
    element_name="${plugin_spec##*:}"
    
    if gst-inspect-1.0 "$element_name" &>/dev/null; then
        echo "  OK: ${plugin_name} (${element_name})"
    else
        echo "  MISSING: ${plugin_name} (${element_name})"
        ERRORS=$((ERRORS + 1))
    fi
done

echo ""

# ── Summary ───────────────────────────────────────────────────────────────────
if [ "$ERRORS" -eq 0 ]; then
    echo "=== All checks passed ==="
    exit 0
else
    echo "=== ${ERRORS} issue(s) found ==="
    echo ""
    echo "Install missing packages:"
    echo "  Debian/Ubuntu: sudo apt install gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav"
    echo "  Fedora:        sudo dnf install gstreamer1-plugins-base gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-libav"
    echo "  Arch:          sudo pacman -S gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav"
    exit 1
fi
