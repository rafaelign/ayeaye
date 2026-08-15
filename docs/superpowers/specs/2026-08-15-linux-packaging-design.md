# Linux Packaging: .deb and AppImage — Design

## Context

AyeAye has shipping-ready assets already committed (`packaging/linux/*.desktop`, `*.metainfo.xml`, icons in every `hicolor` size) but no actual package build process — today the only way to run it is `cargo run -p app` from a source checkout. This spec covers producing a `.deb` for Debian/Ubuntu-family systems and a portable `.AppImage`, plus a GitHub Actions workflow that builds both automatically on a version tag.

This design is grounded in real, verified findings rather than assumptions, using Docker (available on this machine) to actually build and install the package in a clean container instead of just reasoning about it:

- **`cargo deb -p app`** (installed and run against this repo) works out of the box and correctly auto-detects the link-time runtime dependencies via `dpkg-shlibdeps`: `libc6`, `libgbm1`, `libpipewire-0.3-0t64`, `libxcb1`. Default output uses the crate name (`app`) and a placeholder description — needs explicit configuration to produce a real `ayeaye` package.
- **Critical finding, empirically verified**: those auto-detected dependencies are not sufficient to actually run the app. `eframe`/`winit`/`glutin` load several libraries via `dlopen` at runtime rather than linking them at build time, so `dpkg-shlibdeps` — which only inspects the binary's link-time `NEEDED` entries — cannot see them. Installing the `.deb` built with only the auto-detected dependencies into a clean `ubuntu:24.04` container and running it under `xvfb-run` fails immediately with `libXcursor.so.1: cannot open shared object file`. Tracing this through `winit`/`glutin`/`x11-dl`'s source (`x11_link!`/`lib_loading.rs` macros, which name every library they `dlopen`) and then installing candidates and re-testing in the same container until the app launched and stayed running under `xvfb-run` (exit code 124 from `timeout`, not a crash) produced a confirmed-working additional set: `libgl1`, `libegl1`, `libxcursor1`, `libxi6`, `libxrandr2`, `libxinerama1`, `libwayland-client0`, `libwayland-egl1`, `libwayland-cursor0`, `libxkbcommon0`, `libxkbcommon-x11-0`.
- **`crates/app/Cargo.toml` is missing packaging metadata** `cargo-deb` and `crates.io`-style tooling expect: no `description`, `license`, `authors`, `repository`, or `homepage`. `cargo deb` currently warns about all of these and produces a placeholder `Description: [generated from Rust crate app]` and an empty `copyright` file.
- **AppImage tooling** (`linuxdeploy` + `appimagetool`) isn't installed and isn't available via `apt` — both are distributed as standalone executables (themselves AppImages) from GitHub releases, so acquiring them is a download-and-chmod step in the script/workflow, not a package install.
- **The Wayland design's deferred packaging risk note is now directly relevant**: `libpipewire-0.3-0t64`'s own Debian package correctly pulls in `libspa-0.2-modules` transitively (confirmed in the container install log), so the `.deb` path has no PipeWire portability problem at all — `apt`/`dpkg` already solves it. The AppImage path is the one where PipeWire's runtime-loaded SPA plugins remain a real concern (see Goals).

## Goals

- `packaging/linux/build-deb.sh` produces an installable `ayeaye_<version>_amd64.deb` via `cargo deb`, with correct package name, description, copyright, and a `depends` list combining `$auto` with the empirically-verified `dlopen`'d libraries above, plus the existing icons/desktop file/metainfo installed to their standard FHS locations (`/usr/share/icons/hicolor/...`, `/usr/share/applications/...`, `/usr/share/metainfo/...`).
- `packaging/linux/build-appimage.sh` produces a portable `AyeAye-<version>-x86_64.AppImage` via `linuxdeploy` + `appimagetool`, bundling the same general-purpose X11/Wayland client libraries the `.deb` declares, while explicitly excluding GPU-driver-tied libraries (`libGL`, `libEGL`, `libgbm`, Mesa) and `libpipewire` — both categories are things that must match the host (a driver ABI, or a live D-Bus/PipeWire session the portal negotiation talks to regardless of what's bundled) and can't be meaningfully vendored into a portable bundle.
- Both scripts are verified for real in this pass, not just written and assumed to work: the `.deb` via the same clean-container install-and-launch-under-`xvfb-run` method already used during design, the AppImage via extracting it and doing the equivalent.
- `.github/workflows/release.yml` runs both scripts on every push of a `v*` tag and attaches both artifacts to the corresponding GitHub Release, so a tagged release always has downloadable packages without anyone needing to build locally.
- `packaging/linux/README.md` documents prerequisites and how to run each script locally, and the main `README.md`/`README.pt-BR.md` gain a short "Install" pointer to it (or to the Releases page) alongside the existing "Build" (from-source) instructions.

## Non-goals (explicitly out of scope for this pass)

- Publishing to any actual repository or store (a PPA, Flathub, `apt` third-party repo, AUR, etc.) — this pass produces downloadable artifacts attached to GitHub Releases, not a distribution channel. The already-committed `metainfo.xml` even has a comment noting what Flathub submission would still need (screenshots, `<releases>`) — still true, still deferred.
- Non-`amd64` architectures (`arm64`, etc.) — this machine and the empirical dependency verification are `x86_64`-only; cross-compiling and verifying a second architecture is a separate effort.
- Any change to `crates/app`'s actual runtime behavior — this is packaging plumbing only.
- Signing the `.deb` (a GPG-signed `Release`/`Packages` index is only meaningful for an actual `apt` repository, which is out of scope per above) or code-signing the AppImage.
- A coordinated workspace-wide version-bump process — this pass reads whatever `crates/app/Cargo.toml`'s `version` already is (`0.1.0`) for both package formats; deciding how/when that number changes is a separate concern.

## Architecture

Both scripts share one shape: build the release binary once, then assemble each package format from it. The CI workflow calls the exact same two scripts a contributor would run locally — no separate/divergent CI-only logic.

```
cargo build --release -p app
        │
        ├──► build-deb.sh ─────► cargo deb -p app ─────► ayeaye_<ver>_amd64.deb
        │
        └──► build-appimage.sh ─► AppDir assembly
                                   + linuxdeploy (bundle libs, apply excludes)
                                   + appimagetool
                                   └─► AyeAye-<ver>-x86_64.AppImage
```

### `.deb` (`crates/app/Cargo.toml` + `packaging/linux/build-deb.sh`)

Packaging metadata lives in `crates/app/Cargo.toml`'s `[package.metadata.deb]` table — the `cargo-deb` convention, keeping the config next to the crate it packages rather than a separate file that can drift out of sync. Concretely:

- `[package]` gains `description`, `license = "MIT"`, `authors`, `repository`, `homepage` — silences `cargo deb`'s warnings and gives it real values instead of placeholders.
- `[package.metadata.deb]`:
  - `name = "ayeaye"` (the crate is named `app`; the installed package and binary should be `ayeaye`, matching the already-committed `.desktop` file's `Exec=ayeaye`).
  - `depends = "$auto, libgl1, libegl1, libxcursor1, libxi6, libxrandr2, libxinerama1, libwayland-client0, libwayland-egl1, libwayland-cursor0, libxkbcommon0, libxkbcommon-x11-0"` — `$auto` for the link-time-detected set, the rest for the verified `dlopen`'d ones.
  - `section`, `priority = "optional"`, `copyright`, `license-file = ["LICENSE", "4"]`, `extended-description`.
  - `assets`: the release binary renamed to `usr/bin/ayeaye`; the `.desktop` file to `usr/share/applications/`; the `metainfo.xml` to `usr/share/metainfo/`; each `packaging/linux/icons/hicolor/<size>/apps/*.png` to the matching `usr/share/icons/hicolor/<size>/apps/` (a glob per size, reusing the existing directory structure as-is rather than restating every path).
- `build-deb.sh` is a thin wrapper: `cargo deb -p app "$@"` — the real configuration lives in `Cargo.toml` (inspectable, versioned alongside the code it packages) rather than script logic. Its main job beyond that one-liner is the verification step described in Testing.

### AppImage (`packaging/linux/build-appimage.sh`)

1. `cargo build --release -p app`.
2. Assemble an `AppDir` (a conventional directory layout `linuxdeploy` understands): copy the binary to `AppDir/usr/bin/ayeaye`, the `.desktop` file to `AppDir/usr/share/applications/`, and the icon set to `AppDir/usr/share/icons/hicolor/...` — the same destinations as the `.deb`, since `linuxdeploy` expects a normal FHS-shaped tree it then bundles libraries into.
3. Download pinned versions of `linuxdeploy` and `appimagetool` (cached in `packaging/linux/.tools/`, gitignored, re-downloaded only if missing) if not already present — both are standalone executables, not something `apt`/`cargo` installs.
4. Run `linuxdeploy --appdir AppDir --executable AppDir/usr/bin/ayeaye --desktop-file ... --icon-file ...` with `--exclude-library` for the host-dependent set: `libGL*`, `libEGL*`, `libgbm*`, `libdrm*`, Mesa's `libgallium*`, and `libpipewire*`. Everything else `linuxdeploy` finds via `ldd` on the binary gets bundled into `AppDir/usr/lib/`.
5. Run `appimagetool AppDir AyeAye-<version>-x86_64.AppImage`.

The exclude list is the inverse of the `.deb`'s `depends` list by design: the `.deb` *declares* those same GL/PipeWire packages as things `apt` must already satisfy from the host (it doesn't bundle anything, ever); the AppImage *bundles* everything except that same host-tied set. Same underlying judgment — "these must come from the host, not the package" — expressed the only two ways each format allows.

### `.github/workflows/release.yml`

Triggered on `push: tags: ["v*"]`. One `ubuntu-latest` job: checkout, install Rust (stable), install `cargo-deb` (`cargo install cargo-deb`), run `packaging/linux/build-deb.sh` and `packaging/linux/build-appimage.sh`, then upload both artifacts to the GitHub Release for that tag (creating the release if it doesn't already exist, via `softprops/action-gh-release` or equivalent). No separate matrix — one job, one architecture, mirroring the Non-goals above.

## Testing

Both scripts are verified the same way they were designed — actually installing/running the output, not just producing it:

- **`.deb`**: `docker run --rm -v <target/debian>:/deb ubuntu:24.04 bash -c 'apt-get install -y /deb/ayeaye_*.deb xvfb && timeout 5 xvfb-run -a ayeaye; test $? -eq 124'` — a clean container, a real `apt install` (proving the declared `Depends` are complete and correctly named), and a real launch under a virtual X server that must survive being killed by `timeout` rather than exiting on its own (an exit code of anything other than 124 means it crashed or exited early — both failures).
- **AppImage**: extract it (`--appimage-extract`) in a *different* clean container than the one used to build it (so nothing from the build environment's library paths leaks into the "does it actually work standalone" answer), then run the extracted binary the same way, under `xvfb-run`, expecting the same "survives being killed by `timeout`" signal.
- No automated CI test beyond "the build step didn't fail" — actually launching a GUI app under Xvfb in CI is possible but adds real flakiness risk (compositor/GL driver quirks in CI runners) for a check that's already been done manually per release during this design pass; revisit if packaging regressions actually start happening silently.

## Risks / open items

- **The verified `dlopen` dependency list was found by iterative testing on this one machine/container image, not by exhaustively reading every code path** — it's a strong empirical result (the app demonstrably launches and stays running with exactly this set, and demonstrably fails without `libXcursor` alone), but a different desktop environment (a Wayland-only compositor with no XWayland, for instance) could plausibly need something not exercised by this X11-under-Xvfb test. Worth a real end-to-end check on a native Wayland session before calling packaging "done," the same caveat the Wayland capture work already carries.
- **AppImage GL bundling exclusion is a judgment call, not something tested against a second machine with different GPU drivers** — the reasoning (never vendor a driver-tied library into a portable bundle) is standard AppImage practice, but this pass can only verify the AppImage *runs* in this container (software rendering via Mesa's llvmpipe, most likely, same as the `.deb` test), not that it correctly picks up a *different* host's real GPU driver instead of a bundled one, since nothing was bundled to conflict with in the first place.
- **`crates/app/Cargo.toml` currently has no `[package] license`/`authors`/`description`** — adding them is small and uncontroversial, but it's a change to a file nothing else in this design otherwise touches, worth calling out plainly rather than burying in the Architecture section.

## Alternatives considered

- **`cargo-appimage`** (a Cargo subcommand analogous to `cargo-deb`, for AppImages). Rejected: last significant activity and Rust-ecosystem adoption trail `linuxdeploy`/`appimagetool` by a wide margin, and it offers materially less control over the exclude-list decision this design leans on (bundle general X11/Wayland libs, exclude GL/PipeWire) — `linuxdeploy`'s `--exclude-library` flag is exactly the primitive this design needs.
- **Hand-rolling the `.deb` with `dpkg-deb` directly instead of `cargo-deb`.** Rejected: `cargo-deb` already wraps `dpkg-shlibdeps` correctly (verified above) and keeps the packaging manifest declarative in `Cargo.toml`; a hand-rolled script would have to reimplement dependency resolution to get the same correctness, for no benefit.
- **Bundling PipeWire (and its SPA plugins) into the AppImage instead of excluding it.** Rejected: even a fully self-contained PipeWire binary still has to reach the *host's* running PipeWire session and portal services over D-Bus to do anything (screen sharing consent, the actual compositor's screencast implementation) — bundling the client library changes nothing about that dependency, so there's no portability gained, only a larger, potentially-ABI-mismatched artifact.
