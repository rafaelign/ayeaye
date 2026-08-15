use editor::Frame;
use image::RgbaImage;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub enum CaptureError {
    MonitorNotFound(String),
    CaptureFailed(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::MonitorNotFound(msg) => write!(f, "could not find monitor for region: {msg}"),
            CaptureError::CaptureFailed(msg) => write!(f, "screen capture failed: {msg}"),
        }
    }
}

impl std::error::Error for CaptureError {}

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

pub fn bounding_box(monitors: &[Region]) -> Region {
    if monitors.is_empty() {
        return Region { x: 0, y: 0, width: 0, height: 0 };
    }
    let min_x = monitors.iter().map(|m| m.x).min().unwrap();
    let min_y = monitors.iter().map(|m| m.y).min().unwrap();
    let max_x = monitors.iter().map(|m| m.x + m.width as i32).max().unwrap();
    let max_y = monitors.iter().map(|m| m.y + m.height as i32).max().unwrap();
    Region {
        x: min_x,
        y: min_y,
        width: (max_x - min_x) as u32,
        height: (max_y - min_y) as u32,
    }
}

/// Returns the desktop-coordinate bounds of the monitor containing the
/// point `(x, y)`.
pub fn monitor_bounds_at(x: i32, y: i32) -> Result<Region, CaptureError> {
    let monitor = xcap::Monitor::from_point(x, y).map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?;
    Ok(Region {
        x: monitor.x().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?,
        y: monitor.y().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?,
        width: monitor.width().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?,
        height: monitor.height().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?,
    })
}

/// Returns the bounding box of every connected monitor, in desktop
/// coordinates — the size and position the selection overlay viewport
/// needs to cover the whole virtual desktop.
pub fn virtual_screen_bounds() -> Result<Region, CaptureError> {
    let monitors = xcap::Monitor::all().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?;
    let regions = monitors
        .iter()
        .map(|m| {
            Ok(Region {
                x: m.x().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?,
                y: m.y().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?,
                width: m.width().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?,
                height: m.height().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?,
            })
        })
        .collect::<Result<Vec<Region>, CaptureError>>()?;
    Ok(bounding_box(&regions))
}

/// Captures a single still image of every connected monitor, paired with
/// each monitor's desktop-coordinate bounds. Used as a real backdrop for
/// the selection overlay — drawing an actual screenshot instead of relying
/// on OS/compositor window transparency, which isn't reliably honored on
/// every window manager.
pub fn snapshot_monitors() -> Result<Vec<(Region, RgbaImage)>, CaptureError> {
    let monitors = xcap::Monitor::all().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?;
    monitors
        .iter()
        .map(|m| {
            let region = Region {
                x: m.x().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?,
                y: m.y().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?,
                width: m.width().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?,
                height: m.height().map_err(|e| CaptureError::MonitorNotFound(e.to_string()))?,
            };
            let image = m.capture_image().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
            Ok((region, image))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounding_box_of_single_monitor_is_itself() {
        let r = Region { x: 0, y: 0, width: 1920, height: 1080 };
        assert_eq!(bounding_box(&[r]), r);
    }

    #[test]
    fn bounding_box_spans_two_side_by_side_monitors() {
        let a = Region { x: 0, y: 0, width: 1920, height: 1080 };
        let b = Region { x: 1920, y: 0, width: 1280, height: 1024 };
        let bounds = bounding_box(&[a, b]);
        assert_eq!((bounds.x, bounds.y, bounds.width, bounds.height), (0, 0, 3200, 1080));
    }

    #[test]
    fn bounding_box_handles_a_monitor_with_negative_origin() {
        let a = Region { x: -1920, y: 0, width: 1920, height: 1080 };
        let b = Region { x: 0, y: 0, width: 1920, height: 1080 };
        let bounds = bounding_box(&[a, b]);
        assert_eq!((bounds.x, bounds.y, bounds.width, bounds.height), (-1920, 0, 3840, 1080));
    }

    #[test]
    fn bounding_box_of_empty_slice_is_zero_sized() {
        let bounds = bounding_box(&[]);
        assert_eq!((bounds.x, bounds.y, bounds.width, bounds.height), (0, 0, 0, 0));
    }

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
}
