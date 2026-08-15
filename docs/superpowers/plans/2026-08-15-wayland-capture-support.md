# Wayland Capture Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make screen recording work on Wayland sessions (alongside the existing X11 support) from the same binary, using `xcap`'s PipeWire-backed `Monitor::video_recorder()` for continuous capture instead of its single-shot screenshot path, with F9 and multi-monitor area selection gracefully scoped down to what the Wayland portal model actually allows.

**Architecture:** `capture::start_capture` detects the session type once (`XDG_SESSION_TYPE`) and dispatches to one of two loops that both produce the same `Sender<editor::Frame>` output: the existing X11 loop (renamed, unchanged behavior) and a new Wayland loop that streams frames from `xcap::Monitor::video_recorder()`, throttles them to the chosen FPS by dropping frames (a pure, unit-tested gate function), and crops to the selected region client-side. `crates/app` becomes session-aware in two places: the global F9 hotkey is only registered on X11, and the area-selection overlay uses a real fullscreen viewport on a single monitor on Wayland instead of an absolutely-positioned viewport spanning every monitor.

**Tech Stack:** Rust, `xcap` 0.9.8 (already a dependency — Wayland support via `xdg-desktop-portal` + PipeWire, no new crate needed), `egui`/`eframe` 0.36.1 (`ViewportBuilder::with_fullscreen`), `image` 0.25.

**Spec:** `docs/superpowers/specs/2026-08-15-wayland-capture-support-design.md`

## Global Constraints

- Single binary — no separate Wayland build or user-facing toggle. Session type is detected once via `XDG_SESSION_TYPE`, treating exactly the value `"wayland"` as Wayland and everything else (including unset) as X11.
- `capture::start_capture` is the only place that knows about the X11/Wayland split — every other component (recording state, processing, editor) is unchanged, because both loops send the same `editor::Frame` on the same channel.
- F9 is X11-only; Wayland relies solely on the recording indicator's "Parar" button. No portal-based global shortcut in this pass (see spec's Non-goals).
- "Selecionar Área" on Wayland always targets the monitor the app window is on (same restriction "Tela Inteira" already has today) — no multi-monitor picker.
- The X11 code paths must not change behavior. Every task that touches shared code must leave the existing test suite green and, where a task renames/moves X11 logic, must not alter its logic, only its location.
- This plan does not touch `.deb`/AppImage packaging — that's out of scope per the spec (Non-goals).

---

### Task 1: Session type detection

**Files:**
- Modify: `crates/capture/src/lib.rs`
- Test: `crates/capture/src/lib.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `pub enum SessionType { X11, Wayland }` (derives `Debug, Clone, Copy, PartialEq, Eq`), `pub fn session_type() -> SessionType`. Later tasks (3, and `crates/app/src/main.rs` in tasks 5–6) call `capture::session_type()` and compare against `capture::SessionType::X11` / `capture::SessionType::Wayland`.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block at the bottom of `crates/capture/src/lib.rs` (after the existing `bounding_box_of_empty_slice_is_zero_sized` test, still inside the same `mod tests { use super::*; ... }`):

```rust
    #[test]
    fn session_type_from_env_wayland_is_wayland() {
        assert_eq!(session_type_from_env(Some("wayland")), SessionType::Wayland);
    }

    #[test]
    fn session_type_from_env_x11_is_x11() {
        assert_eq!(session_type_from_env(Some("x11")), SessionType::X11);
    }

    #[test]
    fn session_type_from_env_missing_defaults_to_x11() {
        assert_eq!(session_type_from_env(None), SessionType::X11);
    }

    #[test]
    fn session_type_from_env_unknown_value_defaults_to_x11() {
        assert_eq!(session_type_from_env(Some("mir")), SessionType::X11);
    }
```

- [ ] **Step 2: Run the tests to verify they fail to compile**

Run: `cargo test -p capture session_type -- --nocapture`
Expected: FAIL — `SessionType` and `session_type_from_env` are not defined yet.

- [ ] **Step 3: Implement session type detection**

In `crates/capture/src/lib.rs`, insert this right after the `impl std::error::Error for CaptureError {}` block (line 32) and before the `/// Spawns a background thread...` doc comment on `start_capture` (line 34):

```rust
/// Which display server the current desktop session is running. Detected
/// once, via `XDG_SESSION_TYPE` — the same variable the README already
/// tells users to check. `capture::start_capture` is the only place that
/// branches on this; every other component works the same way regardless
/// of which loop actually produced the frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    X11,
    Wayland,
}

fn session_type_from_env(value: Option<&str>) -> SessionType {
    match value {
        Some("wayland") => SessionType::Wayland,
        _ => SessionType::X11,
    }
}

/// Detects the current session type. Anything other than exactly
/// `"wayland"` (including the variable being unset) is treated as X11.
pub fn session_type() -> SessionType {
    session_type_from_env(std::env::var("XDG_SESSION_TYPE").ok().as_deref())
}

```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p capture session_type`
Expected: PASS — 4 tests (`session_type_from_env_wayland_is_wayland`, `session_type_from_env_x11_is_x11`, `session_type_from_env_missing_defaults_to_x11`, `session_type_from_env_unknown_value_defaults_to_x11`).

- [ ] **Step 5: Run the full capture test suite to confirm nothing else broke**

Run: `cargo test -p capture`
Expected: PASS — all tests, including the 4 pre-existing `bounding_box_*` tests.

- [ ] **Step 6: Commit**

```bash
git add crates/capture/src/lib.rs
git commit -m "feat(capture): detect X11 vs Wayland session type"
```

---

### Task 2: FPS frame-gate for the Wayland capture loop

**Files:**
- Modify: `crates/capture/src/lib.rs`
- Test: `crates/capture/src/lib.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `std::time::{Duration, Instant}` (already imported at the top of the file).
- Produces: `fn should_accept(last_accepted: Option<Instant>, now: Instant, interval: Duration) -> bool`. Task 3's Wayland capture loop calls this once per incoming frame.

- [ ] **Step 1: Write the failing tests**

Add to the same `#[cfg(test)] mod tests` block, after the `session_type_from_env_*` tests added in Task 1:

```rust
    #[test]
    fn should_accept_the_first_frame_unconditionally() {
        let now = Instant::now();
        assert!(should_accept(None, now, Duration::from_millis(100)));
    }

    #[test]
    fn should_accept_rejects_a_frame_arriving_before_the_interval_elapsed() {
        let last = Instant::now();
        let now = last + Duration::from_millis(50);
        assert!(!should_accept(Some(last), now, Duration::from_millis(100)));
    }

    #[test]
    fn should_accept_accepts_a_frame_arriving_after_the_interval_elapsed() {
        let last = Instant::now();
        let now = last + Duration::from_millis(150);
        assert!(should_accept(Some(last), now, Duration::from_millis(100)));
    }

    #[test]
    fn should_accept_does_not_burst_catch_up_after_a_long_gap() {
        let interval = Duration::from_millis(100);
        let last = Instant::now();
        let accepted_at = last + Duration::from_millis(500);
        assert!(should_accept(Some(last), accepted_at, interval));
        // Once a frame is accepted, `last_accepted` moves to that frame's
        // time — a frame arriving immediately after is correctly rejected,
        // rather than the gate "catching up" on the gap that preceded it.
        assert!(!should_accept(Some(accepted_at), accepted_at + Duration::from_millis(10), interval));
    }
```

- [ ] **Step 2: Run the tests to verify they fail to compile**

Run: `cargo test -p capture should_accept -- --nocapture`
Expected: FAIL — `should_accept` is not defined yet.

- [ ] **Step 3: Implement the frame gate**

In `crates/capture/src/lib.rs`, insert this right after the `session_type()` function added in Task 1 (and before the `start_capture` doc comment):

```rust
/// FPS throttle for the Wayland capture loop: PipeWire delivers frames at
/// whatever rate the compositor negotiates (commonly close to display
/// refresh rate), so instead of polling at a fixed interval like the X11
/// loop does, each incoming frame is checked against how long it's been
/// since the last frame we kept. `last_accepted` is `None` for the very
/// first frame, which is always accepted.
fn should_accept(last_accepted: Option<Instant>, now: Instant, interval: Duration) -> bool {
    match last_accepted {
        None => true,
        Some(last) => now.duration_since(last) >= interval,
    }
}

```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p capture should_accept`
Expected: PASS — 4 tests.

- [ ] **Step 5: Run the full capture test suite**

Run: `cargo test -p capture`
Expected: PASS — all tests (4 `bounding_box_*` + 4 `session_type_from_env_*` + 4 `should_accept_*`).

- [ ] **Step 6: Commit**

```bash
git add crates/capture/src/lib.rs
git commit -m "feat(capture): add FPS frame-gate for the Wayland capture loop"
```

---

### Task 3: Dispatch capture by session type; implement the Wayland capture loop

**Files:**
- Modify: `crates/capture/src/lib.rs`

**Interfaces:**
- Consumes: `SessionType`/`session_type()` (Task 1), `should_accept` (Task 2), `Region`, `CaptureError`, `editor::Frame`.
- Produces: `pub fn start_capture(...)` keeps its exact existing signature (`region: Region, fps: u32, tx: Sender<Frame>, stop_flag: Arc<AtomicBool>) -> thread::JoinHandle<Result<(), CaptureError>>`) — no caller anywhere else in the codebase needs to change. Internally adds `fn start_capture_x11(...)` (the renamed, unchanged existing loop) and `fn start_capture_wayland(...)`, both `fn(Region, u32, Sender<Frame>, Arc<AtomicBool>) -> Result<(), CaptureError>`.

This task has no new automated tests of its own (it's wiring plus a loop that depends on a real Wayland compositor/portal to exercise) — Step 5 below runs the existing suite to prove the X11 path's behavior is unchanged, and the Testing section of the plan (Task 8) covers manual Wayland verification.

- [ ] **Step 1: Replace `start_capture` with a dispatcher, and rename the existing body to `start_capture_x11`**

In `crates/capture/src/lib.rs`, replace the current `start_capture` function (the `pub fn start_capture(...) -> thread::JoinHandle<...> { thread::spawn(move || { ... }) }` block, currently right after the `should_accept` function added in Task 2) with:

```rust
/// Spawns a background thread that repeatedly captures `region` at roughly
/// `fps` frames per second, sending each captured frame on `tx`, until
/// `stop_flag` is set to `true`. Frames already sent before a capture
/// failure or a stop request remain in the channel — the caller does not
/// lose work already produced. Dispatches to the X11 or Wayland capture
/// loop based on `session_type()` — this is the only place in the codebase
/// that branches on session type for capture purposes.
pub fn start_capture(
    region: Region,
    fps: u32,
    tx: Sender<Frame>,
    stop_flag: Arc<AtomicBool>,
) -> thread::JoinHandle<Result<(), CaptureError>> {
    thread::spawn(move || match session_type() {
        SessionType::X11 => start_capture_x11(region, fps, tx, stop_flag),
        SessionType::Wayland => start_capture_wayland(region, fps, tx, stop_flag),
    })
}

fn start_capture_x11(region: Region, fps: u32, tx: Sender<Frame>, stop_flag: Arc<AtomicBool>) -> Result<(), CaptureError> {
    let monitor = xcap::Monitor::from_point(region.x, region.y)
        .map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?;
    // capture_region takes coordinates local to the monitor's own origin
    // (it only accepts u32, so it cannot represent the global desktop
    // coordinates a monitor left of/above the primary one would have).
    let monitor_x = monitor.x().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?;
    let monitor_y = monitor.y().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?;
    let local_x = (region.x - monitor_x).max(0) as u32;
    let local_y = (region.y - monitor_y).max(0) as u32;
    let interval = Duration::from_millis(1000 / fps.max(1) as u64);
    let start = Instant::now();

    while !stop_flag.load(Ordering::Relaxed) {
        let loop_start = Instant::now();
        let image = monitor
            .capture_region(local_x, local_y, region.width, region.height)
            .map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
        let timestamp_ms = start.elapsed().as_millis() as u64;
        if tx.send(Frame { image, timestamp_ms }).is_err() {
            break; // receiver dropped, nothing more to do
        }
        let elapsed = loop_start.elapsed();
        if elapsed < interval {
            thread::sleep(interval - elapsed);
        }
    }
    Ok(())
}

/// Wayland has no equivalent of X11's cheap, silent `capture_region` — the
/// only continuous-capture primitive is `Monitor::video_recorder()`, which
/// opens an `xdg-desktop-portal` `ScreenCast` session (the user picks a
/// monitor and clicks Share in an OS dialog) and streams frames over
/// PipeWire at whatever rate the compositor negotiates. This loop throttles
/// those frames down to `fps` via `should_accept`, and — since the portal
/// only offers whole-monitor sources — crops each accepted frame down to
/// `region` client-side when the caller asked for less than the full
/// monitor (i.e. "Selecionar Área" rather than "Tela Inteira").
fn start_capture_wayland(region: Region, fps: u32, tx: Sender<Frame>, stop_flag: Arc<AtomicBool>) -> Result<(), CaptureError> {
    let monitor = xcap::Monitor::from_point(region.x, region.y)
        .map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?;
    let monitor_x = monitor.x().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?;
    let monitor_y = monitor.y().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?;
    let monitor_width = monitor.width().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?;
    let monitor_height = monitor.height().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?;
    let local_x = (region.x - monitor_x).max(0) as u32;
    let local_y = (region.y - monitor_y).max(0) as u32;
    let is_full_monitor = local_x == 0 && local_y == 0 && region.width >= monitor_width && region.height >= monitor_height;

    // This is where the OS's "Share your screen" picker dialog appears —
    // the user selects a monitor and clicks Share before frames start
    // flowing on `rx`.
    let (recorder, rx) = monitor.video_recorder().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    recorder.start().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;

    let interval = Duration::from_millis(1000 / fps.max(1) as u64);
    let start = Instant::now();
    let mut last_accepted: Option<Instant> = None;

    while !stop_flag.load(Ordering::Relaxed) {
        // A short timeout instead of a blocking recv keeps this loop
        // responsive to `stop_flag` even if the compositor briefly stops
        // sending frames (e.g. the shared monitor is asleep).
        let raw_frame = match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(frame) => frame,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };

        let now = Instant::now();
        if !should_accept(last_accepted, now, interval) {
            continue;
        }
        last_accepted = Some(now);

        let Some(image) = RgbaImage::from_raw(raw_frame.width, raw_frame.height, raw_frame.raw) else {
            // Buffer size didn't match width*height*4 — a malformed frame
            // from the compositor. Skip it rather than lose the recording.
            continue;
        };
        let image = if is_full_monitor {
            image
        } else {
            // `crop_imm` clips to the image's actual bounds rather than
            // panicking, which matters if the buffer turns out to be in
            // physical pixels while `local_x`/`local_y`/`region` are in
            // logical (scaled) coordinates on a HiDPI setup.
            image::imageops::crop_imm(&image, local_x, local_y, region.width, region.height).to_image()
        };

        let timestamp_ms = start.elapsed().as_millis() as u64;
        if tx.send(Frame { image, timestamp_ms }).is_err() {
            break;
        }
    }

    let _ = recorder.stop();
    Ok(())
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p capture`
Expected: builds cleanly. If `libpipewire-0.3-dev`/`clang` aren't installed on the build machine, this is where `pipewire-sys`'s build script would fail with "Cannot find libpipewire" — see the Global Constraints note in the spec's Risks section; this is a pre-existing transitive requirement of `xcap` 0.9.8, not something this task adds.

- [ ] **Step 3: Run the full capture test suite**

Run: `cargo test -p capture`
Expected: PASS — same 12 tests as after Task 2 (this task adds no new automated tests, just wiring — the X11 loop's logic is unchanged, only moved into a named function).

- [ ] **Step 4: Run the full workspace build**

Run: `cargo build --workspace`
Expected: builds cleanly (confirms `crates/app` still compiles against `capture`'s unchanged public `start_capture` signature).

- [ ] **Step 5: Commit**

```bash
git add crates/capture/src/lib.rs
git commit -m "feat(capture): add a PipeWire-backed Wayland capture loop"
```

---

### Task 4: Single-monitor snapshot for the Wayland selection overlay

**Files:**
- Modify: `crates/capture/src/lib.rs`

**Interfaces:**
- Consumes: `Region`, `CaptureError`.
- Produces: `pub fn snapshot_monitor(region: Region) -> Result<RgbaImage, CaptureError>`. Task 6 (`crates/app/src/main.rs`) calls this for the Wayland selection-overlay backdrop, in place of `snapshot_monitors()` (all monitors).

No new automated tests — this is a thin wrapper around `xcap::Monitor::from_point`/`capture_image`, the same primitives `start_capture_x11` and `snapshot_monitors` already use without dedicated tests (they require a real display to exercise).

- [ ] **Step 1: Add `snapshot_monitor`**

In `crates/capture/src/lib.rs`, insert this right after the existing `snapshot_monitors` function (after its closing `}`, before the `#[cfg(test)]` block):

```rust

/// Captures a single still image of the monitor containing `region`'s
/// origin point. Used as the selection-overlay backdrop on Wayland, where
/// the overlay only ever covers one monitor (the one the app window is
/// on) instead of the whole virtual desktop like on X11 — see
/// `snapshot_monitors` for the X11 equivalent.
pub fn snapshot_monitor(region: Region) -> Result<RgbaImage, CaptureError> {
    let monitor = xcap::Monitor::from_point(region.x, region.y).map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?;
    monitor.capture_image().map_err(|e| CaptureError::CaptureFailed(e.to_string()))
}
```

- [ ] **Step 2: Build and run the full capture test suite**

Run: `cargo build -p capture && cargo test -p capture`
Expected: builds cleanly, same 12 tests still pass (no new tests added this task).

- [ ] **Step 3: Commit**

```bash
git add crates/capture/src/lib.rs
git commit -m "feat(capture): add single-monitor snapshot for the Wayland overlay backdrop"
```

---

### Task 5: Session-aware global hotkey

**Files:**
- Modify: `crates/app/src/main.rs:104-134` (the `struct App` definition and its `impl App { fn new(...) }`), `crates/app/src/main.rs:173-179` (the F9-handling block inside `impl eframe::App for App::ui`)

**Interfaces:**
- Consumes: `capture::session_type()`, `capture::SessionType` (Task 1).
- Produces: `App::_hotkey_manager` becomes `Option<GlobalHotKeyManager>`, `App::toggle_hotkey` becomes `Option<HotKey>`. No other file reads these fields directly (grep confirms `toggle_hotkey`/`_hotkey_manager` are only referenced inside `main.rs`), so this is a self-contained change.

This task has no new automated tests — `App` owns a real OS hotkey registration and an `eframe::App` render loop, neither of which is unit-testable; correctness is verified by the workspace build/test suite (unaffected code paths) plus the manual checklist in Task 7.

- [ ] **Step 1: Make the hotkey fields optional and only register on X11**

In `crates/app/src/main.rs`, replace:

```rust
struct App {
    _hotkey_manager: GlobalHotKeyManager,
    toggle_hotkey: HotKey,
    state: AppState,
    /// Set when a background operation (capture, export) fails partway
    /// through, so the current screen can show a warning without losing
    /// whatever work was already done. Cleared when the user starts a new
    /// attempt.
    last_error: Option<String>,
    /// The app icon, uploaded once at startup and reused everywhere it's
    /// shown in the UI (project screen header, editor top bar) — cheap to
    /// clone, since `TextureHandle` is just a ref-counted handle.
    logo: egui::TextureHandle,
}

impl App {
    fn new(logo: egui::TextureHandle) -> Self {
        let manager = GlobalHotKeyManager::new().expect("failed to create global hotkey manager");
        let toggle_hotkey = HotKey::new(None, Code::F9);
        manager
            .register(toggle_hotkey)
            .expect("failed to register F9 hotkey (is another app using it?)");
        Self {
            _hotkey_manager: manager,
            toggle_hotkey,
            state: AppState::Project(ProjectScreen::default()),
            last_error: None,
            logo,
        }
    }
}
```

with:

```rust
struct App {
    /// `None` on Wayland — `global-hotkey`'s Linux backend is X11-only, so
    /// there's nothing to hold there. The recording indicator's "Parar"
    /// button is the only way to stop a recording on Wayland.
    _hotkey_manager: Option<GlobalHotKeyManager>,
    toggle_hotkey: Option<HotKey>,
    state: AppState,
    /// Set when a background operation (capture, export) fails partway
    /// through, so the current screen can show a warning without losing
    /// whatever work was already done. Cleared when the user starts a new
    /// attempt.
    last_error: Option<String>,
    /// The app icon, uploaded once at startup and reused everywhere it's
    /// shown in the UI (project screen header, editor top bar) — cheap to
    /// clone, since `TextureHandle` is just a ref-counted handle.
    logo: egui::TextureHandle,
}

impl App {
    fn new(logo: egui::TextureHandle) -> Self {
        let (hotkey_manager, toggle_hotkey) = if capture::session_type() == capture::SessionType::X11 {
            let manager = GlobalHotKeyManager::new().expect("failed to create global hotkey manager");
            let toggle_hotkey = HotKey::new(None, Code::F9);
            manager
                .register(toggle_hotkey)
                .expect("failed to register F9 hotkey (is another app using it?)");
            (Some(manager), Some(toggle_hotkey))
        } else {
            (None, None)
        };
        Self {
            _hotkey_manager: hotkey_manager,
            toggle_hotkey,
            state: AppState::Project(ProjectScreen::default()),
            last_error: None,
            logo,
        }
    }
}
```

- [ ] **Step 2: Guard the F9-handling block**

In the same file, inside `impl eframe::App for App { fn ui(...) }`, replace:

```rust
        if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.id == self.toggle_hotkey.id() && event.state == global_hotkey::HotKeyState::Pressed {
                if matches!(self.state, AppState::Recording { .. }) {
                    self.stop_recording(&ctx);
                }
            }
        }
```

with:

```rust
        if let Some(toggle_hotkey) = self.toggle_hotkey {
            if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.id == toggle_hotkey.id() && event.state == global_hotkey::HotKeyState::Pressed {
                    if matches!(self.state, AppState::Recording { .. }) {
                        self.stop_recording(&ctx);
                    }
                }
            }
        }
```

- [ ] **Step 3: Build and run the full workspace test suite**

Run: `cargo build --workspace && cargo test --workspace`
Expected: builds cleanly, all existing tests still pass (this task doesn't add or change any tested logic — `App` isn't unit-tested today).

- [ ] **Step 4: Run the app manually to confirm X11 behavior is unchanged**

Run: `echo $XDG_SESSION_TYPE` — confirm it prints `x11` on this machine, then `cargo run -p app`. Start a full-screen recording, confirm the floating indicator appears, press **F9**, confirm the recording stops and the editor appears. This exercises the `Some(toggle_hotkey)` branch exactly as before.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/main.rs
git commit -m "feat(app): only register the F9 hotkey on X11"
```

---

### Task 6: Wayland-aware selection overlay

**Files:**
- Modify: `crates/app/src/selection_overlay.rs`
- Modify: `crates/app/src/main.rs` (the `AppState::SelectingArea` variant definition, the `Some(ProjectAction::StartAreaSelection) => { ... }` arm, and the `AppState::SelectingArea { ... } => { ... }` render arm)

**Interfaces:**
- Consumes: `capture::session_type()`, `capture::SessionType` (Task 1), `capture::snapshot_monitor` (Task 4), `capture::monitor_bounds_at` (existing), `capture::snapshot_monitors`/`capture::virtual_screen_bounds` (existing, X11 path unchanged).
- Produces: `selection_overlay::show` gains a `fullscreen: bool` parameter between `bounds` and `backdrop`: `pub fn show(ctx: &egui::Context, bounds: capture::Region, fullscreen: bool, backdrop: &[(capture::Region, egui::TextureHandle)], drag_start: &mut Option<(f32, f32)>) -> Option<capture::Region>`. `AppState::SelectingArea` gains a `bounds: capture::Region` field (computed once when entering the state, instead of recomputed every frame via `virtual_screen_bounds()`).

No new automated tests: `region_from_drag`'s existing 4 tests already cover the coordinate math, which this task doesn't change — only which viewport-builder call is made and which bounds/backdrop are computed. The `fullscreen` branch itself needs a real compositor to observe.

- [ ] **Step 1: Add the `fullscreen` parameter to `selection_overlay::show`**

In `crates/app/src/selection_overlay.rs`, update the doc comment and signature:

```rust
/// Renders the selection overlay as its own viewport. On X11, `bounds` is
/// the union of every monitor (see `capture::virtual_screen_bounds`) and
/// the viewport is positioned at its exact desktop coordinates — Wayland
/// doesn't let a client position its own window, so there `fullscreen` is
/// `true` and `bounds` is just the one monitor the app window is on (see
/// `capture::monitor_bounds_at`); the viewport goes fullscreen on whatever
/// output the compositor puts it on instead of being explicitly placed.
/// `drag_start` is owned by the caller (`AppState::SelectingArea`) so it
/// survives across frames, the same pattern `crop_tool`'s callers use.
/// Returns the selected region once the user releases a non-degenerate
/// drag; the caller stops calling this function once that happens, which
/// closes the overlay.
///
/// `backdrop` is a real screenshot of the monitor(s) `bounds` covers
/// (captured once, via `capture::snapshot_monitors` or
/// `capture::snapshot_monitor`, when entering `SelectingArea`), drawn
/// behind the dimming so the user can always see the actual desktop —
/// this doesn't depend on the window manager honoring the viewport's
/// `with_transparent(true)`, which isn't reliably the case on every setup.
pub fn show(
    ctx: &egui::Context,
    bounds: capture::Region,
    fullscreen: bool,
    backdrop: &[(capture::Region, egui::TextureHandle)],
    drag_start: &mut Option<(f32, f32)>,
) -> Option<capture::Region> {
```

- [ ] **Step 2: Branch the viewport builder on `fullscreen`**

In the same function, replace:

```rust
    ctx.show_viewport_immediate(
        viewport_id,
        egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_inner_size([bounds.width as f32, bounds.height as f32])
            .with_position([bounds.x as f32, bounds.y as f32]),
        |ui, _class| {
```

with:

```rust
    let viewport_builder = egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_inner_size([bounds.width as f32, bounds.height as f32]);
    let viewport_builder =
        if fullscreen { viewport_builder.with_fullscreen(true) } else { viewport_builder.with_position([bounds.x as f32, bounds.y as f32]) };
    ctx.show_viewport_immediate(
        viewport_id,
        viewport_builder,
        |ui, _class| {
```

- [ ] **Step 3: Give `AppState::SelectingArea` its own `bounds` field**

In `crates/app/src/main.rs`, in the `enum AppState` definition, replace:

```rust
    SelectingArea {
        fps: u32,
        backdrop: Vec<(capture::Region, egui::TextureHandle)>,
        drag_start: Option<(f32, f32)>,
    },
```

with:

```rust
    SelectingArea {
        fps: u32,
        bounds: capture::Region,
        backdrop: Vec<(capture::Region, egui::TextureHandle)>,
        drag_start: Option<(f32, f32)>,
    },
```

- [ ] **Step 4: Compute `bounds`/`backdrop` per session type when entering `SelectingArea`**

In the same file, replace the `Some(ProjectAction::StartAreaSelection) => { ... }` arm:

```rust
                Some(ProjectAction::StartAreaSelection) => {
                    let fps = screen.fps;
                    // Captured once, up front — used as the overlay's real
                    // backdrop instead of relying on window transparency,
                    // which isn't reliably honored by every window manager.
                    let backdrop = capture::snapshot_monitors()
                        .expect("could not capture a desktop snapshot")
                        .into_iter()
                        .enumerate()
                        .map(|(i, (region, image))| {
                            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                [image.width() as usize, image.height() as usize],
                                image.as_raw(),
                            );
                            let texture =
                                ctx.load_texture(format!("overlay-backdrop-{i}"), color_image, egui::TextureOptions::default());
                            (region, texture)
                        })
                        .collect();
                    self.state = AppState::SelectingArea { fps, backdrop, drag_start: None };
                }
```

with:

```rust
                Some(ProjectAction::StartAreaSelection) => {
                    let fps = screen.fps;
                    // On Wayland the overlay can only cover one monitor (see
                    // `selection_overlay::show`) — the one the app window is
                    // on, same restriction "Tela Inteira" already has. On
                    // X11 it still spans every monitor, as before.
                    let (bounds, snapshots): (capture::Region, Vec<(capture::Region, image::RgbaImage)>) =
                        if capture::session_type() == capture::SessionType::Wayland {
                            let window_center = ctx
                                .input(|i| i.viewport().inner_rect)
                                .expect("window position is unavailable on this platform")
                                .center();
                            let bounds = capture::monitor_bounds_at(window_center.x as i32, window_center.y as i32)
                                .expect("could not determine the monitor under the app window");
                            let image = capture::snapshot_monitor(bounds).expect("could not capture a desktop snapshot");
                            (bounds, vec![(bounds, image)])
                        } else {
                            let bounds = capture::virtual_screen_bounds().expect("could not enumerate monitors");
                            let snapshots = capture::snapshot_monitors().expect("could not capture a desktop snapshot");
                            (bounds, snapshots)
                        };
                    // Captured once, up front — used as the overlay's real
                    // backdrop instead of relying on window transparency,
                    // which isn't reliably honored by every window manager.
                    let backdrop = snapshots
                        .into_iter()
                        .enumerate()
                        .map(|(i, (region, image))| {
                            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                [image.width() as usize, image.height() as usize],
                                image.as_raw(),
                            );
                            let texture =
                                ctx.load_texture(format!("overlay-backdrop-{i}"), color_image, egui::TextureOptions::default());
                            (region, texture)
                        })
                        .collect();
                    self.state = AppState::SelectingArea { fps, bounds, backdrop, drag_start: None };
                }
```

- [ ] **Step 5: Update the `SelectingArea` render arm to use the stored `bounds` and pass `fullscreen`**

In the same file, replace:

```rust
            AppState::SelectingArea { fps, backdrop, drag_start } => {
                let bounds = capture::virtual_screen_bounds().expect("could not enumerate monitors");
                if let Some(region) = selection_overlay::show(&ctx, bounds, backdrop, drag_start) {
                    should_start_region_recording = Some((region, *fps));
                }
            }
```

with:

```rust
            AppState::SelectingArea { fps, bounds, backdrop, drag_start } => {
                let fullscreen = capture::session_type() == capture::SessionType::Wayland;
                if let Some(region) = selection_overlay::show(&ctx, *bounds, fullscreen, backdrop, drag_start) {
                    should_start_region_recording = Some((region, *fps));
                }
            }
```

- [ ] **Step 6: Build and run the full workspace test suite**

Run: `cargo build --workspace && cargo test --workspace`
Expected: builds cleanly, all existing tests pass, including `selection_overlay`'s 4 `region_from_drag` tests (unaffected — the coordinate math they test isn't touched by this task).

- [ ] **Step 7: Run the app manually to confirm X11 behavior is unchanged**

Run: `cargo run -p app`. Click "Selecionar Área", confirm the overlay still covers the whole virtual desktop (or the single monitor, if this machine only has one) exactly as before, drag a region, confirm recording starts scoped to that region. This exercises the `fullscreen == false` branch.

- [ ] **Step 8: Commit**

```bash
git add crates/app/src/selection_overlay.rs crates/app/src/main.rs
git commit -m "feat(app): fullscreen single-monitor selection overlay on Wayland"
```

---

### Task 7: Document Wayland in both READMEs

**Files:**
- Modify: `README.md`
- Modify: `README.pt-BR.md`

**Interfaces:** None — documentation only.

- [ ] **Step 1: Update the Requirements section in `README.md`**

Replace:

```markdown
## Requirements

- Linux with an X11 session (run `echo $XDG_SESSION_TYPE` to confirm — it should print `x11`).
- Stable Rust (`rustup show` to check).
```

with:

```markdown
## Requirements

- Linux with an X11 or Wayland session (run `echo $XDG_SESSION_TYPE` to check which one).
- Stable Rust (`rustup show` to check).
- `libpipewire-0.3-dev` and `clang` installed — needed to build (`xcap`'s Wayland support pulls in PipeWire bindings unconditionally on Linux, even if you end up running on X11).

> [!NOTE]
> On Wayland, starting a recording opens the OS's screen-sharing picker (pick a monitor, click Share) — this is a security boundary of the Wayland `ScreenCast` portal, not something AyeAye can skip. The **F9** stop shortcut only works on X11; on Wayland, use the "Parar" button on the floating recording indicator. "Selecionar Área" on Wayland is limited to the monitor the app window is on.
```

- [ ] **Step 2: Add a Wayland section to the manual checklist in `README.md`**

Replace the closing of the existing checklist:

```markdown
- [ ] Export and open the resulting GIF — confirm it reflects all edits (duplicated frame, crop, blur, text, order).

</details>
```

with:

```markdown
- [ ] Export and open the resulting GIF — confirm it reflects all edits (duplicated frame, crop, blur, text, order).

**On Wayland** (run under a session where `echo $XDG_SESSION_TYPE` prints `wayland`):

- [ ] Full Screen: record, confirm the OS screen-sharing picker appears and recording only starts after picking a monitor and sharing, indicator appears and counts correctly, the "Parar" button on the indicator stops it (F9 is expected to do nothing), editor shows the result.
- [ ] Select Area: overlay fullscreens on the monitor the app window is on, dragging shows the rectangle in real time, the exported/edited frames only cover the dragged region (not the whole monitor).
- [ ] Recording at each FPS preset (8/12/15/20) roughly matches the expected frame count for the recording's duration (allow some slack — the throttle drops frames, it doesn't guarantee an exact count).

</details>
```

- [ ] **Step 3: Make the equivalent edits in `README.pt-BR.md`**

Replace:

```markdown
## Requisitos

- Linux com sessão X11 (rode `echo $XDG_SESSION_TYPE` para confirmar — deve imprimir `x11`).
- Rust estável (`rustup show` para conferir).
```

with:

```markdown
## Requisitos

- Linux com sessão X11 ou Wayland (rode `echo $XDG_SESSION_TYPE` para saber qual).
- Rust estável (`rustup show` para conferir).
- `libpipewire-0.3-dev` e `clang` instalados — necessários para compilar (o suporte a Wayland da `xcap` traz bindings do PipeWire incondicionalmente no Linux, mesmo rodando no X11).

> [!NOTE]
> No Wayland, iniciar uma gravação abre o seletor de compartilhamento de tela do sistema (escolher um monitor, clicar em Compartilhar) — isso é uma barreira de segurança do portal `ScreenCast` do Wayland, não algo que o AyeAye pode pular. O atalho **F9** para parar só funciona no X11; no Wayland, use o botão "Parar" no indicador flutuante de gravação. "Selecionar Área" no Wayland fica limitada ao monitor onde a janela do app está.
```

Replace:

```markdown
- [ ] Exportar e abrir o GIF resultante — confirme que ele reflete todas as edições (frame duplicado, corte, blur, texto, ordem).

</details>
```

with:

```markdown
- [ ] Exportar e abrir o GIF resultante — confirme que ele reflete todas as edições (frame duplicado, corte, blur, texto, ordem).

**No Wayland** (rode numa sessão onde `echo $XDG_SESSION_TYPE` imprime `wayland`):

- [ ] Tela Inteira: grave, confirme que o seletor de compartilhamento do sistema aparece e a gravação só começa depois de escolher um monitor e compartilhar, o indicador aparece e conta corretamente, o botão "Parar" no indicador para a gravação (o F9 não deve fazer nada), o editor mostra o resultado.
- [ ] Selecionar Área: o overlay fica em tela cheia no monitor onde a janela do app está, o arrasto mostra o retângulo em tempo real, os frames exportados/editados cobrem só a região arrastada (não o monitor inteiro).
- [ ] A gravação em cada FPS (8/12/15/20) bate aproximadamente com a contagem de frames esperada pela duração da gravação (com alguma folga — o throttle descarta frames, não garante uma contagem exata).

</details>
```

- [ ] **Step 4: Proofread both files render correctly**

Open both files (or use a Markdown preview) and confirm the new sections render as expected — correct list nesting, the `> [!NOTE]` callout renders as a GitHub-style admonition (matching the existing `> [!IMPORTANT]` callout already in both files).

- [ ] **Step 5: Commit**

```bash
git add README.md README.pt-BR.md
git commit -m "docs: document Wayland support and its known platform differences"
```

---

### Task 8: Final workspace verification

**Files:** None (verification only).

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace`
Expected: builds cleanly, no warnings.

- [ ] **Step 2: Full workspace test suite**

Run: `cargo test --workspace`
Expected: all tests pass — the pre-existing suite (46 tests as of the last recorded count) plus this plan's additions: 4 `session_type_from_env_*` (Task 1) + 4 `should_accept_*` (Task 2) = 8 new tests, 54 total.

- [ ] **Step 3: Manual X11 smoke test (this machine's session)**

Run: `echo $XDG_SESSION_TYPE` to confirm `x11`, then `cargo run -p app`. Do one full "Tela Inteira" recording and one "Selecionar Área" recording end to end (record, stop via F9, edit, export), confirming both behave exactly as before this plan — this is the concrete check that the X11 path truly wasn't altered, not just "the code looks unchanged."

- [ ] **Step 4: Record what still needs verification on a real Wayland session**

This plan's Wayland code paths (the `start_capture_wayland` loop, the fullscreen single-monitor overlay, the portal picker dialog, F9 being silently skipped) cannot be exercised on this X11 development machine. Leave the "On Wayland" checklist items added in Task 7 unchecked, and tell the user directly that those need to be run on a real Wayland session (GNOME and KDE at minimum, per the spec's Risks section on HiDPI/monitor-geometry assumptions) before Wayland support can be considered verified, not just "should work."

- [ ] **Step 5: Confirm a clean git status**

Run: `git status --short`
Expected: empty (everything from Tasks 1–7 already committed).
