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
