# Linux packaging

Two ways to package AyeAye for Linux: a `.deb` for Debian/Ubuntu-family
systems, and a portable `.AppImage` that runs on most other distributions.
Both are `x86_64`-only for now.

## Build dependencies

Building the binary itself (not just packaging it) needs the full set of
X11/Wayland/EGL development headers — a desktop dev machine usually already
has these, but a minimal system (like a fresh CI runner) doesn't. On
Debian/Ubuntu:

```bash
sudo apt-get install -y \
  libpipewire-0.3-dev clang file \
  libegl1-mesa-dev libgl1-mesa-dev libgbm-dev \
  libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev \
  libx11-dev libxext-dev libxrandr-dev libxinerama-dev libxi-dev libxcursor-dev
```

This exact list is what `.github/workflows/release.yml` installs — verified
by building from scratch in a clean `ubuntu:24.04` Docker container and
iterating until both `cargo build --release -p app` and
`build-appimage.sh`'s library bundling stopped failing on a missing package
(`file` and `libxkbcommon-x11-0` in particular are easy to miss: `file` is
required by `appimagetool`, and `libxkbcommon-x11-0` is a separate runtime
package from `libxkbcommon-dev`, needed because `build-appimage.sh` bundles
it from an absolute path on the build machine).

## `.deb`

```bash
cargo install cargo-deb --locked  # once
packaging/linux/build-deb.sh
```

Produces `target/debian/ayeaye_<version>-1_amd64.deb`. Install with
`sudo apt install ./target/debian/ayeaye_*.deb` — `apt` (not `dpkg -i`)
resolves the runtime dependencies automatically.

## AppImage

```bash
packaging/linux/build-appimage.sh
```

Produces `target/appimage/AyeAye-<version>-x86_64.AppImage`. On first run
this downloads `linuxdeploy` and `appimagetool` into
`packaging/linux/.tools/` (gitignored) and reuses them afterward. Run the
resulting file directly (`chmod +x` it first if needed) — no installation
step.

The AppImage does **not** bundle OpenGL/EGL/GBM libraries or PipeWire — it
relies on whatever the host already has, the same way any other Wayland
screen-recording app does (there's no way to "bundle" a live PipeWire
session or a GPU driver into a portable archive; see
`docs/superpowers/specs/2026-08-15-linux-packaging-design.md` for the full
reasoning). A desktop with a graphical session already has these; a bare
minimal container or chroot might not.

## Both packages, on every release

`.github/workflows/release.yml` runs both scripts and attaches the results
to the GitHub Release whenever a `v*` tag is pushed — these scripts are
exactly what that workflow calls, nothing CI-specific happens outside them.
