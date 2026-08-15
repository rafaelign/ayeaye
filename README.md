<p align="center">
  <img src="docs/assets/logo.png" alt="AyeAye logo" width="120" />
</p>

<h1 align="center">AyeAye</h1>
<p align="center">Screen recorder + GIF editor for Linux (X11/Wayland), written in Rust.</p>

<p align="center">
  <a href="README.pt-BR.md">Português (Brasil)</a> ·
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License"></a>
  <img src="https://img.shields.io/badge/rust-stable-orange.svg" alt="Rust stable">
  <img src="https://img.shields.io/badge/platform-Linux%20(X11%2FWayland)-lightgrey.svg" alt="Linux X11/Wayland">
  <a href="https://ko-fi.com/H2H010PKL5"><img src="https://storage.ko-fi.com/cdn/kofi3.png?v=3" alt="Support me on Ko-fi" height="20"></a>
</p>

Records a region of the screen, lets you edit the frames (delete, reorder, crop, blur, annotate with text), and exports a GIF. The name is a reference to the aye-aye, a nocturnal lemur from Madagascar.

> [!NOTE]
> Inspired by [ScreenToGif](https://github.com/NickeManarin/ScreenToGif) — this project is an independent rewrite in Rust, focused on Linux/X11, with no affiliation to the original project.

## Screenshots

<p align="center">
  <img src="docs/assets/screenshot_project.png" alt="Record screen" width="420" /><br/>
  <sub>Record screen — pick the FPS and start a full-screen or area recording.</sub>
</p>
<p align="center">
  <img src="docs/assets/screenshot_editor.png" alt="Editor screen" width="420" /><br/>
  <sub>Editor — toolbar with the recording tools above the preview, filmstrip and status bar below.</sub>
</p>

## Requirements

- Linux with an X11 or Wayland session (run `echo $XDG_SESSION_TYPE` to check which one).
- Stable Rust (`rustup show` to check).
- `libpipewire-0.3-dev` and `clang` installed — needed to build (`xcap`'s Wayland support pulls in PipeWire bindings unconditionally on Linux, even if you end up running on X11).

> [!NOTE]
> On Wayland, starting a recording opens the OS's screen-sharing picker (pick a monitor, click Share) — this is a security boundary of the Wayland `ScreenCast` portal, not something AyeAye can skip. The **F9** stop shortcut only works on X11; on Wayland, use the "Stop" button on the floating recording indicator. "Select Area" on Wayland is limited to the monitor the app window is on.

## Build

```bash
cargo build --workspace
```

## Run

```bash
cargo run -p app
```

## Usage flow

1. On the "Record screen" screen, choose the FPS (8/12/15/20) and click **Full Screen** or **Select Area**.
   - **Full Screen**: records the monitor where the app window is located.
   - **Select Area**: the screen dims — drag a rectangle over the desired region; recording starts once you release.
2. During recording, a floating indicator shows `REC · MM:SS · N frames`. Click the stop button on the indicator, or press **F9**, at any time.
3. The main window comes back to the foreground with a loading screen ("Processing recording...") while thumbnails are prepared in the background, then shows the editor.
4. In the editor: the toolbar above the preview has the recording tools, the preview sits centered below it, and the filmstrip lists all frames (click to select) above a status bar. Pick a tool from the toolbar:
   - **Select**: Duplicate, move (`<`/`>`), delete the current frame.
   - **Crop**: drag over the preview to crop all frames.
   - **Blur**: adjust intensity, drag over the preview to blur a region across all frames.
   - **Text**: type the text, click the preview to position it across all frames.
   - **Play/Pause** loops the frames in the preview.
5. Click **Export**, choose where to save. The editor remains visible (disabled) with an overlaid progress indicator; it releases automatically and shows "Saved to: ..." when done. "< New recording" discards the current session and returns to the initial screen.

## Scope of this version

See `docs/superpowers/specs/2026-08-12-screentogif-rust-linux-design.md` (original MVP), `docs/superpowers/specs/2026-08-13-screentogif-capture-editor-redesign-design.md` (current capture and editor flow), and `docs/superpowers/specs/2026-08-15-wayland-capture-support-design.md` (Wayland support) for the full design. Out of scope for now: webcam, board mode, per-frame delay editing, drag-and-drop reordering in the filmstrip, choosing a specific window to record, export to video/APNG/PSD, save/load project, a portal-based global shortcut for F9 on Wayland.

## Automated tests

```bash
cargo test --workspace
```

> [!IMPORTANT]
> `capture` and the window/viewport parts of `app` (selection overlay, recording indicator, hiding/focusing the main window) don't have automated tests — they depend on a real X11 display. Use the manual checklist below to verify them; see also `crates/capture/examples/manual_capture.rs`.

<details>
<summary><strong>Manual end-to-end checklist</strong></summary>

- [ ] Full Screen: record, indicator appears and counts correctly, F9 stops it, editor shows the result with the main window in the foreground.
- [ ] Select Area: overlay covers the screen, dragging shows the rectangle in real time, recording starts only in the chosen area.
- [ ] In the editor: exercise Select (duplicate/move/delete), Crop, Blur, Text, and Preview, in that order, on the same recording.
- [ ] Export and open the resulting GIF — confirm it reflects all edits (duplicated frame, crop, blur, text, order).
- [ ] Language toggle: switch between EN and PT-BR in the top bar and confirm every screen's text changes in both directions (project screen, recording indicator, editor toolbar/status bar, processing/exporting/done labels).

**On Wayland** (run under a session where `echo $XDG_SESSION_TYPE` prints `wayland`):

- [ ] Full Screen: record, confirm the OS screen-sharing picker appears and recording only starts after picking a monitor and sharing, indicator appears and counts correctly, the "Stop" button on the indicator stops it (F9 is expected to do nothing), editor shows the result.
- [ ] Select Area: overlay fullscreens on the monitor the app window is on, dragging shows the rectangle in real time, the exported/edited frames only cover the dragged region (not the whole monitor).
- [ ] Recording at each FPS preset (8/12/15/20) roughly matches the expected frame count for the recording's duration (allow some slack — the throttle drops frames, it doesn't guarantee an exact count).

</details>
