# Wayland Capture Support — Design

## Context

AyeAye currently only works on X11 (`crates/capture` calls `xcap::Monitor::capture_region`/`capture_image` directly, `global-hotkey`'s Linux backend is X11-only, and the selection overlay/recording indicator position viewports using absolute desktop coordinates). The README lists Wayland as explicitly out of scope.

`xcap` 0.9.8 (already our dependency) has Wayland support built in: it detects the session type at runtime and routes monitor capture through `xdg-desktop-portal` (GNOME Shell's private screenshot D-Bus interface, the freedesktop `Screenshot` portal, or `wlroots`/wayshot, tried in that order) instead of XCB. This means the app *builds and runs* under Wayland today without any code changes — but two things make that a poor experience rather than real support:

1. **Wrong capture primitive for continuous recording.** `xcap`'s Wayland path for `capture_region`/`capture_image` is a *single-shot screenshot* (writes a temp PNG via D-Bus, reads it back) — calling it in a tight per-frame loop, as `capture::start_capture` does today, means a D-Bus round-trip and file I/O for every single frame. `xcap` also exposes a proper continuous-capture primitive, `Monitor::video_recorder()`, which opens a `ScreenCast` portal session and streams raw frames over PipeWire — this is the primitive we need to actually use for Wayland recording. Confirmed by inspecting the built binary: today's `app` binary has no `libpipewire` linkage at all (`readelf -d` shows no `NEEDED` entry for it), because nothing in our code calls `video_recorder()` — the compiler dead-code-eliminates that whole path. Wiring it in will change the binary's runtime dependencies (see Packaging note).
2. **Portal and compositor constraints the X11 code doesn't have to deal with:**
   - The `ScreenCast` portal requires an interactive OS picker dialog (pick a monitor, click Share) every time a recording starts — there is no silent, click-free "capture the monitor under the app window" like `xcap::Monitor::from_point` gives us on X11.
   - The portal only offers whole-monitor sources (`types: 1` = MONITOR) — no arbitrary sub-region capture. "Selecionar Área" has to become "stream the whole monitor, crop each frame client-side to the dragged rectangle."
   - `global-hotkey` 0.8's Linux backend is X11-only (it unconditionally compiles the X11/XGrabKey backend for every Linux target — there's no Wayland branch, just a `no-op` fallback for non-Linux/Windows/macOS targets). F9 will not work on Wayland.
   - Client windows cannot set their own absolute desktop position on Wayland (the compositor owns placement) — the selection overlay's current trick of positioning a borderless viewport to span the exact bounds of the virtual desktop won't work.

This design scopes what "Wayland support" means for v1, given those constraints, decided with the user:
- The portal's per-recording picker dialog is acceptable (it's not avoidable in a portable way — see Alternatives considered).
- F9 is simply unavailable on Wayland; the recording indicator's "Parar" button is the only stop control there.
- "Selecionar Área" is implemented via client-side cropping of the full-monitor stream (not deferred to a later pass).
- The selection overlay, on Wayland, covers only the monitor the app window is on (fullscreen on that output), not the whole virtual desktop like on X11.

## Goals

- Recording works on both X11 and Wayland from a single binary, with the right session detected automatically at startup (`XDG_SESSION_TYPE`) — no separate build, no user-facing toggle.
- "Tela Inteira" and "Selecionar Área" both work on Wayland, at the chosen FPS, using the PipeWire streaming path (not per-frame screenshots).
- The app degrades predictably where the platform genuinely can't do something (F9, absolute overlay positioning) instead of silently misbehaving or panicking.
- Document the new build-time and runtime library dependencies this introduces, so packaging work (`.deb`, AppImage — explicitly out of scope for *this* spec) has accurate requirements to start from.

## Non-goals (explicitly out of scope for this pass)

- Building the `.deb`/AppImage packages themselves — this spec only documents the dependency changes those builds will need to account for.
- A portal-based global shortcut (`org.freedesktop.portal.GlobalShortcuts`) to recover F9 on Wayland — deferred; support across compositors (especially wlroots-based ones like Sway) is inconsistent enough that it isn't worth the added portal/permission complexity for a feature that already has a working alternative (the indicator's stop button).
- A monitor picker for "Selecionar Área" on Wayland when multiple monitors are connected — the overlay always targets the monitor the app window is on, same restriction "Tela Inteira" already has today.
- Fixing the recording indicator's on-screen position on Wayland — its position hint is a no-op there; wherever the compositor places it is accepted as a known platform difference.
- HiDPI/fractional-scaling correctness guarantees — flagged as an implementation-time risk (see Risks), not something this design can verify without a real Wayland+HiDPI session to test against.

## Architecture

Single binary, runtime dispatch. `crates/capture` gains a `SessionType` check (read `XDG_SESSION_TYPE` once, treat exactly `"wayland"` as Wayland, everything else — including unset — as X11, matching the check the README already tells users to run). This is the one place session type is detected; every other component receives already-resolved information (a chosen capture strategy, a monitor to target) rather than re-checking the environment.

```
                     ┌─────────────────┐
                     │ detect session  │  (XDG_SESSION_TYPE)
                     └────────┬────────┘
                 ┌────────────┴────────────┐
                 ▼                         ▼
        X11 capture loop           Wayland capture loop
   (capture_region on a timer)   (Monitor::video_recorder(),
                                  throttled + cropped)
                 └────────────┬────────────┘
                               ▼
                      Sender<editor::Frame>
                 (same channel, same consumer:
                  the rest of the app is unchanged)
```

Both loops produce the exact same output type on the exact same channel, so nothing downstream of `capture::start_capture` (recording state, processing, editor) needs to know which backend is running. This is the key isolation boundary: **`capture::start_capture` stays the only function that knows about the X11/Wayland split.**

## Component changes

### `crates/capture`

- `start_capture` becomes a thin dispatcher: detect session type, call `start_capture_x11` (today's implementation, renamed, otherwise unchanged) or `start_capture_wayland` (new).
- `start_capture_wayland`:
  - Resolves the target `xcap::Monitor` the same way the X11 path does (`Monitor::from_point`), then calls `.video_recorder()` to get a `VideoRecorder` + `Receiver<xcap::video_recorder::Frame>`, and `.start()`s it.
  - Consumes that receiver in a loop. Each incoming frame is passed through a **frame-gate**: a pure function `fn should_accept(last_accepted: Instant, now: Instant, interval: Duration) -> bool` that says whether enough time has passed since the last frame we kept, given the user's chosen FPS. This is the throttle — PipeWire delivers frames at whatever rate the compositor negotiates (commonly close to display refresh rate), and we drop the ones we don't need instead of polling for them like the X11 loop does. Being a pure function of three timestamps/durations, it's unit-testable without any Wayland runtime.
  - If the requested `Region` is smaller than the monitor (i.e. "Selecionar Área", not "Tela Inteira"), each accepted frame is cropped to the region's monitor-local bounds before being wrapped into an `editor::Frame`, using `image::imageops::crop_imm`. Cropping happens after the fps gate, not before, so we don't do the crop work on frames we're about to discard.
  - Honors `stop_flag` the same way the X11 loop does (checked each loop iteration; also calls `VideoRecorder::stop()` on exit so the PipeWire stream and portal session are released instead of leaking).
- `Region`, `CaptureError`, `bounding_box`, `monitor_bounds_at`, `virtual_screen_bounds` are unchanged — monitor enumeration and geometry go through `xcap::Monitor`'s metadata methods (`x()/y()/width()/height()`), which are backend-agnostic already (both X11 and Wayland `ImplMonitor` implementations answer them).
- `snapshot_monitors()` (used today for the X11 selection overlay's backdrop, which spans every monitor) stays as-is for the X11 path. A new `snapshot_monitor(region: Region) -> Result<RgbaImage, CaptureError>` is added for the Wayland path's single-monitor overlay backdrop — same underlying `Monitor::capture_image()` call, just scoped to one monitor instead of all of them.

### `crates/app/src/main.rs`

- Session type is detected once at startup (via the same check `capture` uses — exposed as `capture::session_type()` returning a small `SessionType { X11, Wayland }` enum, so `capture` stays the single source of truth for how detection works) and stored on `App`.
- `GlobalHotKeyManager` construction and F9 registration only happen on X11. On Wayland, the relevant `App` fields become `Option`, and the F9-handling branch in `ui()` is skipped entirely (`if let Some(...)` instead of today's unconditional setup). No panics, no error dialog — F9 quietly isn't a thing on Wayland, matching the design decision that the indicator's button is the only stop control there.
- The full-screen recording path (`monitor_bounds_at` under the app window's center) is unchanged — it's pure monitor geometry, not capture-backend-specific.

### `crates/app/src/selection_overlay.rs`

- Gains a Wayland branch: instead of one borderless viewport positioned with `with_position` to span `virtual_screen_bounds()`, it requests a real fullscreen viewport (`with_fullscreen(true)`, no explicit position) sized to just the monitor the app window is on, using `capture::snapshot_monitor` for the backdrop instead of `capture::snapshot_monitors`.
- The drag-to-rectangle interaction logic itself (mouse-down/mouse-move/mouse-up producing a `Region`) is backend-agnostic and unchanged — only how the overlay window is created and what backdrop it shows differs.

### `crates/app/src/recording_indicator.rs`

No code change. Its `.with_position([40.0, 40.0])` hint is simply ignored by Wayland compositors (they own placement); documented as a known platform difference, not fixed.

## Data flow (Wayland recording, end to end)

1. User clicks "Tela Inteira" or drags a region in the (fullscreen, single-monitor) selection overlay.
2. `App` calls `capture::start_capture(region, fps, tx, stop_flag)`, which detects Wayland and calls `start_capture_wayland`.
3. `start_capture_wayland` resolves the target monitor, opens a `ScreenCast` portal session via `Monitor::video_recorder()` — **this is where the OS picker dialog appears**; the user selects the monitor there and clicks Share (`select_sources`/`start` in `xcap`'s implementation already handle the D-Bus round-trip and PipeWire negotiation).
4. Frames stream in on the `Receiver<xcap::video_recorder::Frame>`. Each is timestamped, gated by `should_accept` against the chosen FPS, optionally cropped to `region`, converted to an `image::RgbaImage`, wrapped in `editor::Frame`, and sent on `tx` — identical shape to what the X11 loop produces, so `Recording`/`Processing`/`Editing` states downstream need no changes.
5. On stop (indicator button only — F9 doesn't fire on Wayland), `stop_flag` is set; the loop exits, calls `VideoRecorder::stop()`, and the thread returns, same as the X11 path's `join()` contract.

## Error handling

- Portal session creation/negotiation failures (user cancels the picker dialog, compositor has no portal backend, PipeWire isn't running) surface as a `CaptureError` variant through the existing `Result<(), CaptureError>` the capture thread already returns — `App::stop_recording`'s existing "A gravação parou antes do esperado: {e}" handling covers this without changes; no new UI needed.
- `Monitor::video_recorder()` failing at the very start of recording (before any frames were ever captured) goes through the same path — the user sees the error message and lands back with zero frames, same as any other immediate capture failure today.

## Testing

- `should_accept` (the FPS-gate pure function) gets unit tests covering: first frame always accepted, a frame arriving before the interval elapsed is rejected, one arriving after is accepted, and back-to-back frames after a long gap only accept one (no burst catch-up).
- Everything else introduced here — portal negotiation, PipeWire streaming, fullscreen overlay placement — depends on a real Wayland compositor and portal implementation and cannot be exercised in CI, exactly like the existing X11 viewport/capture code today. The manual end-to-end checklist in both READMEs gains a parallel "On Wayland" section mirroring the existing X11 checklist (record full screen, record a selected area, confirm FPS is roughly respected, confirm F9 does nothing and the indicator's button is the only way to stop, confirm the exported GIF reflects the recording).

## Risks / open items to verify during implementation

- **HiDPI/fractional scaling**: PipeWire frame buffers may be in physical pixels while `Monitor::x()/y()/width()/height()` may report logical (scaled) coordinates on some compositors. If so, cropping a selected region requires scaling the rectangle by `Monitor::scale_factor()` before applying it to the raw buffer. This can't be verified on the X11-only machine this design was written on — needs checking against a real HiDPI Wayland session, GNOME and KDE at minimum, during implementation.
- **Monitor geometry reliability on Wayland**: `monitor_bounds_at`/`Monitor::from_point` are assumed to work the same as on X11 (compositors do expose `wl_output` geometry), but this is an assumption to confirm manually, not something proven by reading `xcap`'s source alone.
- **Packaging impact (informational only, not built here)**: once `Monitor::video_recorder()` is actually called from reachable code, `libpipewire-0.3` linkage is no longer dead-code-eliminated — `libpipewire-0.3-dev` (and `clang`/`libclang` for `pipewire-sys`'s `bindgen` step) becomes a real build-time requirement, and `libpipewire-0.3-0` becomes a runtime dependency the `.deb` needs to declare. For an AppImage, PipeWire commonly loads SPA plugins from the host system at runtime (`/usr/lib/.../spa-0.2/`) — bundling `libpipewire-0.3.so` alone may not be sufficient for a fully self-contained AppImage; this is a known rough edge for portable Linux screen-capture tools (OBS Studio's packaging has the same issue) and will need its own investigation when packaging is actually tackled.

## Alternatives considered

- **Skip the portal picker via GNOME's private `org.gnome.Shell.Screenshot`-style API for continuous capture too.** Rejected: no such private continuous-capture API exists (GNOME Shell's private interface is screenshot-only, not video); the only continuous-capture mechanism on Wayland, across all compositors, is the standard `ScreenCast` portal, which always requires the interactive picker by design (it's a deliberate security boundary in the protocol, not a GNOME-specific quirk).
- **Ship a separate Wayland-only binary/build.** Rejected: doubles the build matrix and packaging work for no real benefit, since runtime detection is cheap and reliable (`XDG_SESSION_TYPE`), and the two code paths already converge on the same `Sender<editor::Frame>` interface.
