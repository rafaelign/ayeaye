#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TOOLS_DIR="$SCRIPT_DIR/.tools"
BUILD_DIR="$REPO_ROOT/target/appimage"
APPDIR="$BUILD_DIR/AppDir"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_ROOT/crates/app/Cargo.toml" | head -1)"

LINUXDEPLOY="$TOOLS_DIR/linuxdeploy-x86_64.AppImage"
APPIMAGETOOL="$TOOLS_DIR/appimagetool-x86_64.AppImage"

mkdir -p "$TOOLS_DIR"

if [ ! -x "$LINUXDEPLOY" ]; then
    echo "Downloading linuxdeploy..."
    curl -fsSL -o "$LINUXDEPLOY" \
        https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
    chmod +x "$LINUXDEPLOY"
fi

if [ ! -x "$APPIMAGETOOL" ]; then
    echo "Downloading appimagetool..."
    curl -fsSL -o "$APPIMAGETOOL" \
        https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x "$APPIMAGETOOL"
fi

echo "Building release binary..."
cd "$REPO_ROOT"
cargo build --release -p app

echo "Assembling AppDir..."
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications"
cp "$REPO_ROOT/target/release/app" "$APPDIR/usr/bin/ayeaye"
cp "$REPO_ROOT/packaging/linux/io.github.rafaelign.AyeAye.desktop" "$APPDIR/usr/share/applications/"
cp -r "$REPO_ROOT/packaging/linux/icons" "$APPDIR/usr/share/icons"

echo "Running linuxdeploy..."
# eframe/winit/glutin load these libraries via dlopen at runtime, so
# linuxdeploy's own ldd-based auto-detection (same limitation as
# dpkg-shlibdeps, see build-deb.sh) misses them — bundle them explicitly.
# The dlopen'd GL/EGL/GBM/PipeWire libraries are deliberately NOT bundled:
# they're tied to the host's GPU driver or a live system service, so
# vendoring them would either be useless (PipeWire still has to reach the
# host's real session over D-Bus regardless) or actively break rendering
# on a host with different drivers. Confirmed empirically: this exact
# --library/--exclude-library set produces an AppImage that launches and
# stays running under xvfb-run in a clean container with only
# libgl1/libegl1/libpipewire-0.3-0 installed from the host, and fails
# with a clear "libpipewire-0.3.so.0: cannot open shared object file"
# error (not a silent misbehavior) if those host packages are absent.
"$LINUXDEPLOY" --appimage-extract-and-run \
    --appdir "$APPDIR" \
    --executable "$APPDIR/usr/bin/ayeaye" \
    --desktop-file "$REPO_ROOT/packaging/linux/io.github.rafaelign.AyeAye.desktop" \
    --icon-file "$REPO_ROOT/packaging/linux/icons/hicolor/256x256/apps/io.github.rafaelign.AyeAye.png" \
    --library /usr/lib/x86_64-linux-gnu/libXcursor.so.1 \
    --library /usr/lib/x86_64-linux-gnu/libXi.so.6 \
    --library /usr/lib/x86_64-linux-gnu/libXrandr.so.2 \
    --library /usr/lib/x86_64-linux-gnu/libXinerama.so.1 \
    --library /usr/lib/x86_64-linux-gnu/libwayland-client.so.0 \
    --library /usr/lib/x86_64-linux-gnu/libwayland-egl.so.1 \
    --library /usr/lib/x86_64-linux-gnu/libwayland-cursor.so.0 \
    --library /usr/lib/x86_64-linux-gnu/libxkbcommon.so.0 \
    --library /usr/lib/x86_64-linux-gnu/libxkbcommon-x11.so.0 \
    --exclude-library 'libGL*' \
    --exclude-library 'libEGL*' \
    --exclude-library 'libgbm*' \
    --exclude-library 'libpipewire*'

echo "Running appimagetool..."
OUTPUT="$BUILD_DIR/AyeAye-$VERSION-x86_64.AppImage"
rm -f "$OUTPUT"
"$APPIMAGETOOL" --appimage-extract-and-run "$APPDIR" "$OUTPUT"

echo "Built $OUTPUT"
