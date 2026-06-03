#!/bin/bash
# build-deb.sh — Build a .deb package for TuneCraft
#
# Prerequisites (Debian/Ubuntu):
#   sudo apt-get install -y cargo libasound2-dev libgtk-3-dev libssl-dev dpkg-dev
#
# Usage:
#   ./dist/build-deb.sh          # builds release binary and .deb
#   ./dist/build-deb.sh --skip-build  # only packages an existing binary

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# v0.14.0: Derive version from workspace Cargo.toml instead of hardcoding it.
VERSION="$(grep '^version' "${PROJECT_ROOT}/Cargo.toml" | head -1 | sed 's/.*= *"\(.*\)"/\1/')"
ARCH="$(dpkg --print-architecture)"
PKG_NAME="tunecraft"
PKG_DIR="${PROJECT_ROOT}/target/deb/${PKG_NAME}_${VERSION}_${ARCH}"

SKIP_BUILD=false
if [[ "${1:-}" == "--skip-build" ]]; then
    SKIP_BUILD=true
fi

echo "=== TuneCraft .deb Packager ==="
echo "Version: ${VERSION}"
echo "Architecture: ${ARCH}"
echo "Package dir: ${PKG_DIR}"
echo ""

# Step 1: Build release binary
if [[ "$SKIP_BUILD" == false ]]; then
    echo "[1/5] Building release binary..."
    cd "$PROJECT_ROOT"
    cargo build --release --workspace
else
    echo "[1/5] Skipping build (--skip-build)"
fi

BINARY="${PROJECT_ROOT}/target/release/tunecraft"
if [[ ! -f "$BINARY" ]]; then
    echo "ERROR: Release binary not found at ${BINARY}"
    echo "Run without --skip-build or build manually with: cargo build --release"
    exit 1
fi

# Step 2: Create package directory structure
echo "[2/5] Creating package structure..."
rm -rf "${PKG_DIR}"
mkdir -p "${PKG_DIR}/DEBIAN"
mkdir -p "${PKG_DIR}/usr/bin"
mkdir -p "${PKG_DIR}/usr/share/applications"
mkdir -p "${PKG_DIR}/usr/share/icons/hicolor/256x256/apps"
mkdir -p "${PKG_DIR}/usr/share/doc/tunecraft"
mkdir -p "${PKG_DIR}/usr/share/man/man1"

# Step 3: Install files
echo "[3/5] Installing files..."
cp "${BINARY}" "${PKG_DIR}/usr/bin/tunecraft"
chmod 755 "${PKG_DIR}/usr/bin/tunecraft"

cp "${PROJECT_ROOT}/dist/tunecraft.desktop" \
   "${PKG_DIR}/usr/share/applications/tunecraft.desktop"
chmod 644 "${PKG_DIR}/usr/share/applications/tunecraft.desktop"

# v0.14.0: Install desktop icon (was missing — blocker #6 fix)
if [[ -f "${PROJECT_ROOT}/crates/tc-ui/icon.png" ]]; then
    cp "${PROJECT_ROOT}/crates/tc-ui/icon.png" \
       "${PKG_DIR}/usr/share/icons/hicolor/256x256/apps/tunecraft.png"
    chmod 644 "${PKG_DIR}/usr/share/icons/hicolor/256x256/apps/tunecraft.png"
fi

# Copy LICENSE if it exists
if [[ -f "${PROJECT_ROOT}/LICENSE" ]]; then
    cp "${PROJECT_ROOT}/LICENSE" "${PKG_DIR}/usr/share/doc/tunecraft/"
fi

# Copy README if it exists
if [[ -f "${PROJECT_ROOT}/README.md" ]]; then
    cp "${PROJECT_ROOT}/README.md" "${PKG_DIR}/usr/share/doc/tunecraft/"
fi

# Install man page
if [[ -f "${PROJECT_ROOT}/dist/tunecraft.1" ]]; then
    gzip -9 -c "${PROJECT_ROOT}/dist/tunecraft.1" \
        > "${PKG_DIR}/usr/share/man/man1/tunecraft.1.gz"
    chmod 644 "${PKG_DIR}/usr/share/man/man1/tunecraft.1.gz"
fi

# Step 4: Create control file
echo "[4/5] Creating control file..."
cat > "${PKG_DIR}/DEBIAN/control" <<EOF
Package: ${PKG_NAME}
Version: ${VERSION}
Section: sound
Priority: optional
Architecture: ${ARCH}
Depends: libasound2 (>= 1.1), libgtk-3-0 (>= 3.22), libssl3 (>= 3.0)
Recommends: rtkit
Suggests: pulseaudio-utils
Maintainer: TuneCraft Contributors <hello@tunecraft.app>
Description: Production-grade offline music player with DSP processing
 TuneCraft is a high-fidelity music player featuring a parametric
 equalizer, loudness normalization, real-time audio analysis,
 MPRIS D-Bus integration, and local play-history journaling.
 .
 Supported formats: MP3, FLAC, OGG/Vorbis, WAV, AAC.
 Features: 10-band parametric EQ, stereo enhancer, convolution
 reverb/room correction, resampler with quality profiles,
 BPM detection, mood classification, and synced lyrics.
 .
 v1.0.2: Bug-fix release — add ScrobbleService::is_available() (fixes
 smoke test compile error), remove orphaned md-5 dependency, upgrade
 icon to 256x256, bump Flatpak runtime to 24.08.
 .
 v1.0.1: Bug-fix release — correct man page version/date, fix media key
 platform notes, remove obsolete Last.fm references, add Cargo.lock for
 reproducible CI, align test feature flags.
 .
 v1.0.0: First stable release with CI/CD, comprehensive test coverage,
 database version fix, and all pre-release bug fixes consolidated.
 .
 v0.31.0: Fix stale doc comments, update broken smoke test, add missing
 v0.30.0 changelog entry, bump version.
 .
 v0.30.0: Consolidate build_resampler helper, fix GainProcessor clamping,
 remove dead code, clean up doc comments, update versioning.
 .
 v0.29.0: Split engine into submodules, eliminate crossfade heap
 allocations, extract build_resampler helper, add 26 new tests.
Homepage: https://tunecraft.app
License: MIT
EOF

# Create postinst script to update desktop database
cat > "${PKG_DIR}/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications 2>/dev/null || true
fi
EOF
chmod 755 "${PKG_DIR}/DEBIAN/postinst"

# Create postrm script
cat > "${PKG_DIR}/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications 2>/dev/null || true
fi
EOF
chmod 755 "${PKG_DIR}/DEBIAN/postrm"

# Step 5: Build the .deb package
echo "[5/5] Building .deb package..."
dpkg-deb --build --root-owner-group "${PKG_DIR}"

DEB_FILE="${PKG_DIR}.deb"
if [[ -f "$DEB_FILE" ]]; then
    echo ""
    echo "=== Package built successfully ==="
    echo "Output: ${DEB_FILE}"
    echo "Size: $(du -h "$DEB_FILE" | cut -f1)"
    echo ""
    echo "Install with: sudo dpkg -i ${DEB_FILE}"
else
    echo "ERROR: Failed to build .deb package"
    exit 1
fi
