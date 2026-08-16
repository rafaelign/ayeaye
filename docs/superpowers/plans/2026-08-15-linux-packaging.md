# Linux Packaging (.deb, AppImage) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce an installable `.deb` and a portable `.AppImage` for AyeAye, buildable with one script each locally and automatically via GitHub Actions on every version tag.

**Architecture:** `cargo-deb` builds the `.deb` straight from metadata declared in `crates/app/Cargo.toml`. A new `packaging/linux/build-appimage.sh` assembles a conventional `AppDir`, then runs `linuxdeploy` (bundling the app's runtime-`dlopen`'d X11/Wayland libraries explicitly, since neither `linuxdeploy` nor `dpkg-shlibdeps` can see `dlopen` calls) and `appimagetool` to produce the final `.AppImage`. Both formats exclude the same set of host-tied libraries (GL/EGL/GBM/PipeWire) — the `.deb` declares them as `Depends`, the AppImage explicitly excludes them from bundling — for the same reason: a driver ABI or a live system service can't be meaningfully vendored. A GitHub Actions workflow runs the same two scripts on `v*` tag pushes and attaches both artifacts to the release.

**Tech Stack:** `cargo-deb` (Rust, installed via `cargo install`), `linuxdeploy` + `appimagetool` (standalone executables downloaded from GitHub releases, not `apt`-installable), Docker (already available on this machine) for real install/launch verification, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-08-15-linux-packaging-design.md`

## Global Constraints

- `x86_64` only — this pass doesn't cover other architectures (see spec Non-goals).
- Every runtime dependency claim in this plan (both the `.deb`'s `depends` list and the AppImage's explicit `--library`/`--exclude-library` set) was verified empirically during design by installing/running the actual artifact in a clean `ubuntu:24.04` Docker container — not assumed from reading library names. Re-verify the same way (Task 1's and Task 2's verification steps) rather than trusting the list blindly if anything about the build changes.
- GL/EGL/GBM/PipeWire are never bundled into either package format — always a runtime dependency on whatever the host already has (see spec Architecture and Alternatives Considered for why).
- No config file, `.desktop` entry, or icon gets duplicated or hand-copied outside of what's declared in `crates/app/Cargo.toml`'s `[package.metadata.deb]` (for the `.deb`) or `packaging/linux/build-appimage.sh` (for the AppImage) — both formats source the same already-committed `packaging/linux/*.desktop`, `*.metainfo.xml`, and `icons/` tree, never a copy of them.

---

### Task 1: `.deb` packaging

**Files:**
- Modify: `crates/app/Cargo.toml`
- Create: `packaging/linux/build-deb.sh`

**Interfaces:**
- Produces: running `packaging/linux/build-deb.sh` from anywhere produces `target/debian/ayeaye_<version>-1_amd64.deb`. No other task depends on this file's internals — Task 3 (CI) just calls the script.

This task has no automated tests of its own — Debian packaging metadata and a real `.deb` install can't be unit tested. Its "test" is Step 5 below: a real install into a clean container, the same empirical method used during design.

- [ ] **Step 1: Add packaging metadata fields to `[package]`**

In `crates/app/Cargo.toml`, replace:

```toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"
```

with:

```toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"
description = "Screen recorder and GIF editor for Linux"
license = "MIT"
authors = ["Rafael Crispim Ignácio <rafael.nacio@gmail.com>"]
repository = "https://github.com/rafaelign/ayeaye"
homepage = "https://github.com/rafaelign/ayeaye"
```

- [ ] **Step 2: Add the `[package.metadata.deb]` table**

Append to the end of `crates/app/Cargo.toml`:

```toml

[package.metadata.deb]
name = "ayeaye"
maintainer = "Rafael Crispim Ignácio <rafael.nacio@gmail.com>"
copyright = "2026, Rafael Crispim Ignácio <rafael.nacio@gmail.com>"
license-file = ["../../LICENSE", "4"]
extended-description = """\
AyeAye records a region of the screen, lets you edit the frames (delete, \
reorder, crop, blur, annotate with text), and exports a GIF. Supports both \
X11 and Wayland sessions."""
section = "video"
priority = "optional"
# $auto covers link-time dependencies (via dpkg-shlibdeps); the rest are
# libraries eframe/winit/glutin load via dlopen at runtime, which
# dpkg-shlibdeps can't see because it only inspects link-time NEEDED
# entries — confirmed by installing a $auto-only build into a clean
# ubuntu:24.04 container and watching it fail on libXcursor.so.1 at
# startup, then adding libraries back one at a time until it launched
# and stayed running under xvfb-run.
depends = "$auto, libgl1, libegl1, libxcursor1, libxi6, libxrandr2, libxinerama1, libwayland-client0, libwayland-egl1, libwayland-cursor0, libxkbcommon0, libxkbcommon-x11-0"
assets = [
    ["target/release/app", "usr/bin/ayeaye", "755"],
    ["../../packaging/linux/io.github.rafaelign.AyeAye.desktop", "usr/share/applications/io.github.rafaelign.AyeAye.desktop", "644"],
    ["../../packaging/linux/io.github.rafaelign.AyeAye.metainfo.xml", "usr/share/metainfo/io.github.rafaelign.AyeAye.metainfo.xml", "644"],
    ["../../packaging/linux/icons/hicolor/16x16/apps/io.github.rafaelign.AyeAye.png", "usr/share/icons/hicolor/16x16/apps/io.github.rafaelign.AyeAye.png", "644"],
    ["../../packaging/linux/icons/hicolor/32x32/apps/io.github.rafaelign.AyeAye.png", "usr/share/icons/hicolor/32x32/apps/io.github.rafaelign.AyeAye.png", "644"],
    ["../../packaging/linux/icons/hicolor/48x48/apps/io.github.rafaelign.AyeAye.png", "usr/share/icons/hicolor/48x48/apps/io.github.rafaelign.AyeAye.png", "644"],
    ["../../packaging/linux/icons/hicolor/64x64/apps/io.github.rafaelign.AyeAye.png", "usr/share/icons/hicolor/64x64/apps/io.github.rafaelign.AyeAye.png", "644"],
    ["../../packaging/linux/icons/hicolor/128x128/apps/io.github.rafaelign.AyeAye.png", "usr/share/icons/hicolor/128x128/apps/io.github.rafaelign.AyeAye.png", "644"],
    ["../../packaging/linux/icons/hicolor/256x256/apps/io.github.rafaelign.AyeAye.png", "usr/share/icons/hicolor/256x256/apps/io.github.rafaelign.AyeAye.png", "644"],
    ["../../packaging/linux/icons/hicolor/512x512/apps/io.github.rafaelign.AyeAye.png", "usr/share/icons/hicolor/512x512/apps/io.github.rafaelign.AyeAye.png", "644"],
]
```

`assets` source paths are resolved relative to `crates/app/` (the crate's own manifest directory), which is why the desktop file, metainfo, icons, and `LICENSE` are all reached via `../../` — confirmed empirically during design (a test asset entry pointing at `../../LICENSE` from this exact file correctly picked up the workspace-root `LICENSE`).

- [ ] **Step 3: Build and inspect the package**

Run: `cargo install cargo-deb --locked` (skip if already installed — `cargo deb --version` to check), then `cargo deb -p app -v`.

Expected: no warnings about missing `license`/`description`/copyright; a line `Depends libc6 (...), libgbm1 (...), libpipewire-0.3-0t64 (...), libxcb1 (...)` (the `$auto` part — exact version floors may differ by build machine, that's fine); output path `target/debian/ayeaye_0.1.0-1_amd64.deb`.

Run: `dpkg-deb -c target/debian/ayeaye_0.1.0-1_amd64.deb` and confirm it lists `./usr/bin/ayeaye`, `./usr/share/applications/io.github.rafaelign.AyeAye.desktop`, `./usr/share/metainfo/io.github.rafaelign.AyeAye.metainfo.xml`, and all 7 icon sizes under `./usr/share/icons/hicolor/`.

Run: `dpkg-deb -f target/debian/ayeaye_0.1.0-1_amd64.deb Depends` and confirm the full combined list (both the `$auto` packages and the 11 manually-added ones) is present.

- [ ] **Step 4: Create the wrapper script**

Create `packaging/linux/build-deb.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if ! command -v cargo-deb >/dev/null 2>&1; then
    echo "cargo-deb not found. Install it with: cargo install cargo-deb --locked" >&2
    exit 1
fi

cd "$REPO_ROOT"
cargo deb -p app "$@"
```

Run: `chmod +x packaging/linux/build-deb.sh`.

- [ ] **Step 5: Verify the real install in a clean container**

Run:

```bash
docker run --rm -v "$(pwd)/target/debian:/deb" ubuntu:24.04 bash -c '
    apt-get update -qq >/dev/null 2>&1
    apt-get install -y -qq /deb/ayeaye_0.1.0-1_amd64.deb xvfb >/dev/null 2>&1
    timeout 5 xvfb-run -a ayeaye
    echo "exit code: $?"
'
```

Expected: `exit code: 124` — killed by `timeout` after 5 seconds of running, not crashed. Any other exit code (especially a fast one) means either the install failed or the binary crashed on startup — investigate before moving on; don't treat a fast exit as "probably fine."

- [ ] **Step 6: Commit**

```bash
git add crates/app/Cargo.toml packaging/linux/build-deb.sh
git commit -m "feat(packaging): add .deb build via cargo-deb"
```

---

### Task 2: AppImage packaging

**Files:**
- Create: `packaging/linux/build-appimage.sh`
- Modify: `.gitignore`

**Interfaces:**
- Consumes: `crates/app/Cargo.toml`'s `version` field (Task 1 didn't change its value, still `0.1.0`), the same `packaging/linux/*.desktop`/`*.metainfo.xml`/`icons/` assets Task 1 uses.
- Produces: running `packaging/linux/build-appimage.sh` produces `target/appimage/AyeAye-<version>-x86_64.AppImage`. Downloads `linuxdeploy`/`appimagetool` into `packaging/linux/.tools/` on first run, reusing them on subsequent runs.

No automated tests — same reasoning as Task 1; Step 4 below is the real verification.

- [ ] **Step 1: Ignore the downloaded tool cache**

In `.gitignore`, add:

```
/packaging/linux/.tools
```

- [ ] **Step 2: Create the build script**

Create `packaging/linux/build-appimage.sh`:

```bash
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
```

Run: `chmod +x packaging/linux/build-appimage.sh`.

- [ ] **Step 3: Run it**

Run: `packaging/linux/build-appimage.sh`

Expected: downloads `linuxdeploy`/`appimagetool` into `packaging/linux/.tools/` (only on the first run), builds the release binary, and produces `target/appimage/AyeAye-0.1.0-x86_64.AppImage`. Check `ls -la packaging/linux/.tools/ target/appimage/`.

- [ ] **Step 4: Verify the real launch in a clean container, separate from the build environment**

Run:

```bash
docker run --rm -v "$(pwd)/target/appimage/AyeAye-0.1.0-x86_64.AppImage:/AyeAye.AppImage:ro" ubuntu:24.04 bash -c '
    apt-get update -qq >/dev/null 2>&1
    apt-get install -y -qq xvfb libgl1 libegl1 libpipewire-0.3-0 >/dev/null 2>&1
    cp /AyeAye.AppImage /tmp/a.AppImage && chmod +x /tmp/a.AppImage
    cd /tmp && ./a.AppImage --appimage-extract >/dev/null 2>&1
    timeout 5 xvfb-run -a /tmp/squashfs-root/AppRun
    echo "exit code: $?"
'
```

Expected: `exit code: 124` (same reasoning as Task 1 Step 5 — killed by the timeout, not crashed).

Optionally, confirm the exclude list is actually load-bearing by re-running the same container command without `libgl1 libegl1 libpipewire-0.3-0` in the `apt-get install` line — expect a clear `error while loading shared libraries: libpipewire-0.3.so.0: cannot open shared object file` rather than a silent hang or a different failure, confirming the AppImage genuinely depends on the host for exactly the libraries it was designed to.

- [ ] **Step 5: Commit**

```bash
git add packaging/linux/build-appimage.sh .gitignore
git commit -m "feat(packaging): add AppImage build via linuxdeploy + appimagetool"
```

---

### Task 3: GitHub Actions release workflow

**Files:**
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: `packaging/linux/build-deb.sh` and `packaging/linux/build-appimage.sh` (Tasks 1–2) — calls them exactly as a local run would, no CI-only branching logic.

No automated tests possible for a GitHub Actions workflow outside of GitHub's own runners — this task's verification is Step 2 (a syntax/structure check available locally) plus a note that the real end-to-end run only happens on an actual tag push, which isn't part of this plan (pushing a tag is a release action, not a build/implementation one).

- [ ] **Step 1: Create the workflow**

Create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags:
      - "v*"

jobs:
  linux-packages:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable

      - name: Install cargo-deb
        run: cargo install cargo-deb --locked

      - name: Install packaging build dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y libpipewire-0.3-dev clang

      - name: Build .deb
        run: packaging/linux/build-deb.sh

      - name: Build AppImage
        run: packaging/linux/build-appimage.sh

      - name: Upload release artifacts
        uses: softprops/action-gh-release@v2
        with:
          files: |
            target/debian/*.deb
            target/appimage/*.AppImage
```

`libpipewire-0.3-dev`/`clang` are needed to *build* the binary at all (the same requirement already documented in the main README's Requirements section, from the Wayland support work) — `cargo-deb`/`linuxdeploy` package the already-built binary, they don't need those dev headers themselves.

- [ ] **Step 2: Validate the YAML locally**

Run: `python3 -c "import yaml, sys; yaml.safe_load(open('.github/workflows/release.yml'))" ` (or any available YAML parser) to catch indentation/syntax errors before pushing — this can't validate GitHub Actions semantics, only that the file parses as valid YAML.

Expected: no output (parses cleanly).

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "feat(ci): build and publish .deb/AppImage on version tags"
```

---

### Task 4: Documentation

**Files:**
- Create: `packaging/linux/README.md`
- Modify: `README.md`
- Modify: `README.pt-BR.md`

**Interfaces:** None — documentation only.

- [ ] **Step 1: Write the packaging README**

Create `packaging/linux/README.md`:

```markdown
# Linux packaging

Two ways to package AyeAye for Linux: a `.deb` for Debian/Ubuntu-family
systems, and a portable `.AppImage` that runs on most other distributions.
Both are `x86_64`-only for now.

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
```

- [ ] **Step 2: Point to it from the main READMEs**

In `README.md`, replace:

```markdown
## Build

```bash
cargo build --workspace
```
```

with:

```markdown
## Install

Prebuilt `.deb` and `.AppImage` packages are attached to each
[Release](https://github.com/rafaelign/ayeaye/releases) — see
`packaging/linux/README.md` for how they're built.

## Build from source

```bash
cargo build --workspace
```
```

- [ ] **Step 3: Same addition in `README.pt-BR.md`**

Replace:

```markdown
## Build

```bash
cargo build --workspace
```
```

with:

```markdown
## Instalar

Pacotes `.deb` e `.AppImage` prontos ficam anexados a cada
[Release](https://github.com/rafaelign/ayeaye/releases) — veja
`packaging/linux/README.md` para saber como são gerados.

## Build a partir do código-fonte

```bash
cargo build --workspace
```
```

- [ ] **Step 4: Commit**

```bash
git add packaging/linux/README.md README.md README.pt-BR.md
git commit -m "docs: document the .deb/AppImage packaging process"
```

---

### Task 5: Final verification

**Files:** None (verification only).

- [ ] **Step 1: Full workspace build and test suite (unaffected by this plan's changes, confirm no regressions)**

Run: `cargo build --workspace && cargo test --workspace`
Expected: builds cleanly, all 67 tests pass — this plan touches packaging only, not application code.

- [ ] **Step 2: Rebuild both packages from a clean state**

Run:

```bash
rm -rf target/debian target/appimage
packaging/linux/build-deb.sh
packaging/linux/build-appimage.sh
ls -la target/debian/*.deb target/appimage/*.AppImage
```

Expected: both files exist and are non-empty.

- [ ] **Step 3: Re-run both container verifications one more time against these fresh builds**

Repeat Task 1 Step 5 and Task 2 Step 4 exactly, against the freshly-built `.deb` and `.AppImage` from Step 2 above (adjust the version in the filename if it's changed since those tasks ran). Both must still show `exit code: 124`.

- [ ] **Step 4: Confirm a clean git status**

Run: `git status --short`
Expected: empty (everything from Tasks 1–4 already committed; `target/` and `packaging/linux/.tools/` are both gitignored so the freshly-built artifacts from Step 2 don't show up).
