# ScreenToGif Rust/Linux MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Linux/X11 desktop app that records a screen region, lets the user delete/reorder/crop the captured frames, and exports the result as a GIF.

**Architecture:** A Cargo workspace with four crates: `editor` (pure in-memory frame-list logic, no GUI/X11 deps), `capture` (X11 screen capture via `xcap` on a background thread), `encoder` (GIF encoding via `gifski`), and `app` (the `egui`/`eframe` binary that ties selection → recording → editing → export together as a small state machine).

**Tech Stack:** Rust (edition 2021), `egui`/`eframe`, `xcap`, `global-hotkey`, `gifski`, `rfd`, `image`.

## Global Constraints

- Target platform: Linux with an X11 session (per spec — confirmed as the user's environment). Wayland is out of scope.
- Single-session flow only: record → edit → export → close. No project save/load to disk.
- Editor operations limited to: delete frame, reorder frame, crop (applied to all frames).
- Export format limited to GIF only, via `gifski`.
- No webcam, no board mode, no image filters/overlays — all out of scope per spec.
- Prefer `cargo add <crate>` over hand-picked version numbers in `Cargo.toml`, so dependency versions resolve to whatever is actually current and compiles, rather than a guessed version that may not exist.

---

## File Structure

```
Cargo.toml                        # workspace root
crates/
  editor/
    Cargo.toml
    src/lib.rs                    # Frame, FrameList, CropRect, EditorError
  capture/
    Cargo.toml
    src/lib.rs                    # Region, CaptureError, start_capture()
    examples/manual_capture.rs    # manual verification tool (no real X11 display in CI)
  encoder/
    Cargo.toml
    src/lib.rs                    # EncodeError, encode_gif()
    tests/encode_gif.rs           # automated integration test (no display needed)
  app/
    Cargo.toml
    src/main.rs                   # AppState machine, eframe entry point
    src/selection.rs              # pure window-rect -> capture::Region conversion
    src/editor_screen.rs          # thumbnail strip, delete/reorder UI
    src/crop_tool.rs              # pure drag-rect -> editor::CropRect conversion
    src/export_screen.rs          # background export job + progress polling
README.md                         # build/run instructions + manual E2E checklist
```

---

### Task 1: `editor` crate — Frame, FrameList, delete, reorder

**Files:**
- Create: `Cargo.toml`
- Create: `crates/editor/Cargo.toml`
- Create: `crates/editor/src/lib.rs`
- Test: inline `#[cfg(test)] mod tests` in `crates/editor/src/lib.rs`

**Interfaces:**
- Produces: `pub struct Frame { pub image: image::RgbaImage, pub timestamp_ms: u64 }` (derives `Clone`)
- Produces: `pub enum EditorError { IndexOutOfBounds, InvalidCropRect }` (derives `Debug, PartialEq, Eq`; implements `Display` + `Error`)
- Produces: `pub struct FrameList` with `new(Vec<Frame>) -> Self`, `len(&self) -> usize`, `is_empty(&self) -> bool`, `frames(&self) -> &[Frame]`, `delete(&mut self, index: usize) -> Result<(), EditorError>`, `reorder(&mut self, from: usize, to: usize) -> Result<(), EditorError>`

- [ ] **Step 1: Create the workspace root `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["crates/editor"]
```

- [ ] **Step 2: Create `crates/editor/Cargo.toml`**

```toml
[package]
name = "editor"
version = "0.1.0"
edition = "2021"

[dependencies]
image = "0.25"
```

- [ ] **Step 3: Write `crates/editor/src/lib.rs` with type stubs and failing tests**

```rust
use image::RgbaImage;

#[derive(Clone)]
pub struct Frame {
    pub image: RgbaImage,
    pub timestamp_ms: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditorError {
    IndexOutOfBounds,
    InvalidCropRect,
}

impl std::fmt::Display for EditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditorError::IndexOutOfBounds => write!(f, "frame index out of bounds"),
            EditorError::InvalidCropRect => write!(f, "crop rect is invalid for this frame"),
        }
    }
}

impl std::error::Error for EditorError {}

pub struct FrameList {
    frames: Vec<Frame>,
}

impl FrameList {
    pub fn new(frames: Vec<Frame>) -> Self {
        unimplemented!()
    }

    pub fn len(&self) -> usize {
        unimplemented!()
    }

    pub fn is_empty(&self) -> bool {
        unimplemented!()
    }

    pub fn frames(&self) -> &[Frame] {
        unimplemented!()
    }

    pub fn delete(&mut self, index: usize) -> Result<(), EditorError> {
        unimplemented!()
    }

    pub fn reorder(&mut self, from: usize, to: usize) -> Result<(), EditorError> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(tag: u8) -> Frame {
        // 1x1 pixel whose red channel encodes an identifying tag, so tests
        // can assert on frame identity after reordering/deleting.
        Frame {
            image: RgbaImage::from_pixel(1, 1, image::Rgba([tag, 0, 0, 255])),
            timestamp_ms: tag as u64 * 100,
        }
    }

    fn tags(list: &FrameList) -> Vec<u8> {
        list.frames().iter().map(|f| f.image.get_pixel(0, 0).0[0]).collect()
    }

    #[test]
    fn new_list_reports_correct_len() {
        let list = FrameList::new(vec![make_frame(1), make_frame(2), make_frame(3)]);
        assert_eq!(list.len(), 3);
        assert!(!list.is_empty());
    }

    #[test]
    fn empty_list_is_empty() {
        let list = FrameList::new(vec![]);
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
    }

    #[test]
    fn delete_removes_the_frame_at_index() {
        let mut list = FrameList::new(vec![make_frame(1), make_frame(2), make_frame(3)]);
        list.delete(1).unwrap();
        assert_eq!(tags(&list), vec![1, 3]);
    }

    #[test]
    fn delete_out_of_bounds_returns_error() {
        let mut list = FrameList::new(vec![make_frame(1)]);
        assert_eq!(list.delete(5), Err(EditorError::IndexOutOfBounds));
    }

    #[test]
    fn reorder_moves_frame_to_new_position() {
        let mut list = FrameList::new(vec![make_frame(1), make_frame(2), make_frame(3)]);
        list.reorder(0, 2).unwrap();
        assert_eq!(tags(&list), vec![2, 3, 1]);
    }

    #[test]
    fn reorder_out_of_bounds_returns_error() {
        let mut list = FrameList::new(vec![make_frame(1), make_frame(2)]);
        assert_eq!(list.reorder(0, 9), Err(EditorError::IndexOutOfBounds));
    }
}
```

- [ ] **Step 4: Run the tests, verify they fail**

Run: `cargo test -p editor`
Expected: 6 tests run, all FAIL (panic: `not implemented`).

- [ ] **Step 5: Implement `FrameList` for real**

Replace the five `unimplemented!()` bodies:

```rust
impl FrameList {
    pub fn new(frames: Vec<Frame>) -> Self {
        Self { frames }
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub fn delete(&mut self, index: usize) -> Result<(), EditorError> {
        if index >= self.frames.len() {
            return Err(EditorError::IndexOutOfBounds);
        }
        self.frames.remove(index);
        Ok(())
    }

    pub fn reorder(&mut self, from: usize, to: usize) -> Result<(), EditorError> {
        if from >= self.frames.len() || to >= self.frames.len() {
            return Err(EditorError::IndexOutOfBounds);
        }
        let frame = self.frames.remove(from);
        self.frames.insert(to, frame);
        Ok(())
    }
}
```

- [ ] **Step 6: Run the tests, verify they pass**

Run: `cargo test -p editor`
Expected: `test result: ok. 6 passed; 0 failed`

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/editor
git commit -m "feat(editor): add FrameList with delete and reorder"
```

---

### Task 2: `editor` crate — crop

**Files:**
- Modify: `crates/editor/src/lib.rs`

**Interfaces:**
- Consumes: `EditorError::InvalidCropRect` (already defined in Task 1), `image::imageops::crop_imm`
- Produces: `pub struct CropRect { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }`
- Produces: `FrameList::crop(&mut self, rect: CropRect) -> Result<(), EditorError>`

- [ ] **Step 1: Add the failing tests and the `CropRect` type + stub method**

Add above `impl FrameList` (or anywhere at module scope):

```rust
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
```

Add to `impl FrameList`:

```rust
pub fn crop(&mut self, rect: CropRect) -> Result<(), EditorError> {
    unimplemented!()
}
```

Add to the `tests` module:

```rust
#[test]
fn crop_shrinks_every_frame_to_the_requested_rect() {
    let mut list = FrameList::new(vec![
        Frame {
            image: RgbaImage::from_fn(4, 4, |x, y| image::Rgba([x as u8, y as u8, 0, 255])),
            timestamp_ms: 0,
        },
        Frame {
            image: RgbaImage::from_fn(4, 4, |x, y| image::Rgba([x as u8, y as u8, 0, 255])),
            timestamp_ms: 100,
        },
    ]);
    list.crop(CropRect { x: 1, y: 1, width: 2, height: 2 }).unwrap();
    for frame in list.frames() {
        assert_eq!(frame.image.dimensions(), (2, 2));
        // pixel (0,0) of the cropped image is pixel (1,1) of the original
        assert_eq!(frame.image.get_pixel(0, 0), &image::Rgba([1, 1, 0, 255]));
    }
}

#[test]
fn crop_rect_outside_frame_bounds_returns_error() {
    let mut list = FrameList::new(vec![Frame {
        image: RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255])),
        timestamp_ms: 0,
    }]);
    let err = list.crop(CropRect { x: 3, y: 3, width: 4, height: 4 }).unwrap_err();
    assert_eq!(err, EditorError::InvalidCropRect);
}

#[test]
fn crop_with_zero_size_returns_error() {
    let mut list = FrameList::new(vec![Frame {
        image: RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255])),
        timestamp_ms: 0,
    }]);
    let err = list.crop(CropRect { x: 0, y: 0, width: 0, height: 2 }).unwrap_err();
    assert_eq!(err, EditorError::InvalidCropRect);
}
```

- [ ] **Step 2: Run the tests, verify the three new ones fail**

Run: `cargo test -p editor crop`
Expected: 3 tests run, all FAIL (panic: `not implemented`).

- [ ] **Step 3: Implement `crop`**

```rust
pub fn crop(&mut self, rect: CropRect) -> Result<(), EditorError> {
    if rect.width == 0 || rect.height == 0 {
        return Err(EditorError::InvalidCropRect);
    }
    for frame in &self.frames {
        if rect.x + rect.width > frame.image.width() || rect.y + rect.height > frame.image.height() {
            return Err(EditorError::InvalidCropRect);
        }
    }
    for frame in &mut self.frames {
        frame.image = image::imageops::crop_imm(&frame.image, rect.x, rect.y, rect.width, rect.height)
            .to_image();
    }
    Ok(())
}
```

- [ ] **Step 4: Run the tests, verify they pass**

Run: `cargo test -p editor`
Expected: `test result: ok. 9 passed; 0 failed`

- [ ] **Step 5: Commit**

```bash
git add crates/editor
git commit -m "feat(editor): add crop operation to FrameList"
```

---

### Task 3: `capture` crate — X11 screen capture

**Files:**
- Modify: `Cargo.toml` (add `crates/capture` to workspace members)
- Create: `crates/capture/Cargo.toml`
- Create: `crates/capture/src/lib.rs`
- Create: `crates/capture/examples/manual_capture.rs`

**Interfaces:**
- Consumes: `editor::Frame`
- Produces: `pub struct Region { pub x: i32, pub y: i32, pub width: u32, pub height: u32 }`
- Produces: `pub enum CaptureError { MonitorNotFound(String), CaptureFailed(String) }` (implements `Display` + `Error`)
- Produces: `pub fn start_capture(region: Region, fps: u32, tx: std::sync::mpsc::Sender<editor::Frame>, stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>) -> std::thread::JoinHandle<Result<(), CaptureError>>`

This crate needs a live X11 display to do anything meaningful, so per the spec it is verified manually rather than with `cargo test`.

- [ ] **Step 1: Add `crates/capture` to the workspace and create its `Cargo.toml`**

Update root `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/editor", "crates/capture"]
```

Then, from the repo root, add the dependencies (lets Cargo resolve real current versions instead of hand-typed guesses):

```bash
mkdir -p crates/capture/src
cd crates/capture
cargo init --lib --name capture
cargo add --path ../editor
cargo add xcap
cd ../..
```

- [ ] **Step 2: Write `crates/capture/src/lib.rs`**

```rust
use editor::Frame;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

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

/// Spawns a background thread that repeatedly captures `region` at roughly
/// `fps` frames per second, sending each captured frame on `tx`, until
/// `stop_flag` is set to `true`. Frames already sent before a capture
/// failure or a stop request remain in the channel — the caller does not
/// lose work already produced.
pub fn start_capture(
    region: Region,
    fps: u32,
    tx: Sender<Frame>,
    stop_flag: Arc<AtomicBool>,
) -> thread::JoinHandle<Result<(), CaptureError>> {
    thread::spawn(move || {
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
    })
}
```

- [ ] **Step 3: Write the manual verification example**

Create `crates/capture/examples/manual_capture.rs`:

```rust
use capture::{start_capture, Region};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    let (tx, rx) = channel();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let region = Region { x: 0, y: 0, width: 320, height: 240 };
    let handle = start_capture(region, 5, tx, stop_flag.clone());

    thread::sleep(Duration::from_secs(2));
    stop_flag.store(true, Ordering::Relaxed);

    let frames: Vec<_> = rx.iter().collect();
    println!("captured {} frames", frames.len());
    for (i, frame) in frames.iter().enumerate() {
        frame.image.save(format!("/tmp/capture_frame_{i}.png")).unwrap();
    }
    handle.join().unwrap().unwrap();
}
```

- [ ] **Step 4: Run it and manually verify the captured images**

Run: `cargo run -p capture --example manual_capture`
Expected: prints `captured N frames` where N is roughly `2 seconds * 5 fps` (around 8-10, exact count varies with scheduling). Then open `/tmp/capture_frame_0.png` in an image viewer and confirm it shows the top-left 320x240 corner of the real screen at the time it ran.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/capture
git commit -m "feat(capture): add X11 region capture via xcap"
```

---

### Task 4: `encoder` crate — GIF encoding via gifski

**Files:**
- Modify: `Cargo.toml` (add `crates/encoder` to workspace members)
- Create: `crates/encoder/Cargo.toml`
- Create: `crates/encoder/src/lib.rs`
- Test: `crates/encoder/tests/encode_gif.rs`

**Interfaces:**
- Consumes: `editor::Frame { image: image::RgbaImage, timestamp_ms: u64 }`
- Produces: `pub enum EncodeError { NoFrames, Encoding(String), Io(std::io::Error) }` (implements `Display` + `Error`)
- Produces: `pub fn encode_gif<P: AsRef<Path>>(frames: &[editor::Frame], output_path: P, progress: impl FnMut(usize, usize) + Send + 'static) -> Result<(), EncodeError>`

- [ ] **Step 1: Add `crates/encoder` to the workspace and create its `Cargo.toml`**

Update root `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/editor", "crates/capture", "crates/encoder"]
```

```bash
mkdir -p crates/encoder/src
cd crates/encoder
cargo init --lib --name encoder
cargo add --path ../editor
cargo add gifski imgref rgb
cargo add gif --dev
cd ../..
```

- [ ] **Step 2: Write `crates/encoder/src/lib.rs` with a stub `encode_gif`**

```rust
use editor::Frame;
use gifski::progress::ProgressReporter;
use std::fs::File;
use std::path::Path;

#[derive(Debug)]
pub enum EncodeError {
    NoFrames,
    Encoding(String),
    Io(std::io::Error),
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::NoFrames => write!(f, "cannot encode an empty frame list"),
            EncodeError::Encoding(msg) => write!(f, "gif encoding failed: {msg}"),
            EncodeError::Io(err) => write!(f, "could not write gif file: {err}"),
        }
    }
}

impl std::error::Error for EncodeError {}

pub fn encode_gif<P: AsRef<Path>>(
    frames: &[Frame],
    output_path: P,
    progress: impl FnMut(usize, usize) + Send + 'static,
) -> Result<(), EncodeError> {
    unimplemented!()
}
```

- [ ] **Step 3: Write the failing integration test**

Create `crates/encoder/tests/encode_gif.rs`:

```rust
use editor::Frame;
use encoder::{encode_gif, EncodeError};
use image::RgbaImage;
use std::sync::{Arc, Mutex};

#[test]
fn encode_gif_writes_a_valid_multi_frame_gif() {
    let frames = vec![
        Frame { image: RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255])), timestamp_ms: 0 },
        Frame { image: RgbaImage::from_pixel(4, 4, image::Rgba([0, 255, 0, 255])), timestamp_ms: 200 },
        Frame { image: RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 255, 255])), timestamp_ms: 400 },
    ];
    let output = std::env::temp_dir().join("encoder_test_output.gif");

    let progress_calls = Arc::new(Mutex::new(0usize));
    let progress_calls_clone = progress_calls.clone();

    encode_gif(&frames, &output, move |current, total| {
        assert!(current <= total);
        *progress_calls_clone.lock().unwrap() += 1;
    })
    .expect("encode_gif should succeed");

    assert!(*progress_calls.lock().unwrap() > 0);

    let file = std::fs::File::open(&output).unwrap();
    let mut decoder = gif::DecodeOptions::new();
    let mut reader = decoder.read_info(file).unwrap();
    let mut frame_count = 0;
    while reader.read_next_frame().unwrap().is_some() {
        frame_count += 1;
    }
    assert_eq!(frame_count, 3);

    std::fs::remove_file(&output).ok();
}

#[test]
fn encode_gif_rejects_empty_frame_list() {
    let output = std::env::temp_dir().join("encoder_test_empty.gif");
    let err = encode_gif(&[], &output, |_, _| {}).unwrap_err();
    assert!(matches!(err, EncodeError::NoFrames));
}
```

- [ ] **Step 4: Run the tests, verify they fail**

Run: `cargo test -p encoder`
Expected: both tests FAIL (panic: `not implemented`).

- [ ] **Step 5: Implement `encode_gif`**

```rust
struct CallbackProgress<F: FnMut(usize, usize)> {
    total: usize,
    current: usize,
    callback: F,
}

impl<F: FnMut(usize, usize) + Send> ProgressReporter for CallbackProgress<F> {
    fn increase(&mut self) -> bool {
        self.current += 1;
        (self.callback)(self.current, self.total);
        true
    }

    fn done(&mut self, _msg: &str) {}
}

pub fn encode_gif<P: AsRef<Path>>(
    frames: &[Frame],
    output_path: P,
    progress: impl FnMut(usize, usize) + Send + 'static,
) -> Result<(), EncodeError> {
    if frames.is_empty() {
        return Err(EncodeError::NoFrames);
    }

    let settings = gifski::Settings::default();
    let (mut collector, writer) =
        gifski::new(settings).map_err(|e| EncodeError::Encoding(e.to_string()))?;

    let frames_owned: Vec<Frame> = frames.to_vec();

    let collect_handle = std::thread::spawn(move || -> Result<(), EncodeError> {
        for (i, frame) in frames_owned.iter().enumerate() {
            let pixels: Vec<rgb::RGBA8> = frame
                .image
                .pixels()
                .map(|p| rgb::RGBA8::new(p[0], p[1], p[2], p[3]))
                .collect();
            let img = imgref::Img::new(pixels, frame.image.width() as usize, frame.image.height() as usize);
            let pts = frame.timestamp_ms as f64 / 1000.0;
            collector
                .add_frame_rgba(i, img, pts)
                .map_err(|e| EncodeError::Encoding(e.to_string()))?;
        }
        Ok(())
    });

    let file = File::create(output_path).map_err(EncodeError::Io)?;
    let mut reporter = CallbackProgress { total: frames.len(), current: 0, callback: progress };
    writer
        .write(file, &mut reporter)
        .map_err(|e| EncodeError::Encoding(e.to_string()))?;

    collect_handle.join().expect("collector thread panicked")?;
    Ok(())
}
```

- [ ] **Step 6: Run the tests, verify they pass**

Run: `cargo test -p encoder`
Expected: `test result: ok. 2 passed; 0 failed`

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/encoder
git commit -m "feat(encoder): add GIF encoding via gifski"
```

---

### Task 5: `app` crate scaffold — selection overlay window + hotkey registration

**Files:**
- Modify: `Cargo.toml` (add `crates/app` to workspace members)
- Create: `crates/app/Cargo.toml`
- Create: `crates/app/src/main.rs`
- Create: `crates/app/src/selection.rs`

**Interfaces:**
- Produces: `pub fn region_from_window_rect(x: f32, y: f32, width: f32, height: f32) -> capture::Region` in `selection.rs`

- [ ] **Step 1: Add `crates/app` to the workspace and create its `Cargo.toml`**

Update root `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/editor", "crates/capture", "crates/encoder", "crates/app"]
```

```bash
mkdir -p crates/app/src
cd crates/app
cargo init --bin --name app
cargo add --path ../editor
cargo add --path ../capture
cargo add --path ../encoder
cargo add eframe egui global-hotkey rfd image
cd ../..
```

- [ ] **Step 2: Write `crates/app/src/selection.rs` with a stub and failing tests**

```rust
use capture::Region;

pub fn region_from_window_rect(x: f32, y: f32, width: f32, height: f32) -> Region {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_window_rect_to_region() {
        let region = region_from_window_rect(100.4, 50.6, 320.2, 240.9);
        assert_eq!(region.x, 100);
        assert_eq!(region.y, 51);
        assert_eq!(region.width, 320);
        assert_eq!(region.height, 241);
    }

    #[test]
    fn clamps_degenerate_size_to_at_least_one_pixel() {
        let region = region_from_window_rect(0.0, 0.0, 0.0, 0.0);
        assert_eq!(region.width, 1);
        assert_eq!(region.height, 1);
    }
}
```

- [ ] **Step 3: Run the tests, verify they fail**

Run: `cargo test -p app`
Expected: both tests FAIL (panic: `not implemented`).

- [ ] **Step 4: Implement `region_from_window_rect`**

```rust
pub fn region_from_window_rect(x: f32, y: f32, width: f32, height: f32) -> Region {
    Region {
        x: x.round() as i32,
        y: y.round() as i32,
        width: width.round().max(1.0) as u32,
        height: height.round().max(1.0) as u32,
    }
}
```

- [ ] **Step 5: Run the tests, verify they pass**

Run: `cargo test -p app`
Expected: `test result: ok. 2 passed; 0 failed`

- [ ] **Step 6: Write `crates/app/src/main.rs`**

```rust
mod selection;

use eframe::egui;
use global_hotkey::{
    hotkey::{Code, HotKey},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};

struct App {
    _hotkey_manager: GlobalHotKeyManager,
    toggle_hotkey: HotKey,
}

impl Default for App {
    fn default() -> Self {
        let manager = GlobalHotKeyManager::new().expect("failed to create global hotkey manager");
        let toggle_hotkey = HotKey::new(None, Code::F9);
        manager
            .register(toggle_hotkey)
            .expect("failed to register F9 hotkey (is another app using it?)");
        Self { _hotkey_manager: manager, toggle_hotkey }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.id == self.toggle_hotkey.id() {
                println!("F9 event: {:?}", event.state);
            }
        }
        ui.ctx().request_repaint(); // keep polling the hotkey channel every frame

        egui::CentralPanel::default().show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label("Posicione esta janela sobre a região a gravar. F9 inicia a gravação.");
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_resizable(true)
            .with_transparent(true)
            .with_inner_size([320.0, 240.0]),
        ..Default::default()
    };

    eframe::run_native("ScreenToGif", options, Box::new(|_cc| Ok(Box::new(App::default()))))
}
```

(Note: the `eframe`/`egui` version resolved by `cargo add` at implementation time was 0.36.1, whose `eframe::App` trait uses `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame)` instead of the older `fn update(&mut self, ctx: &egui::Context, frame: &mut Frame)`, and panel `.show()` methods take `&mut Ui` instead of `&Context` to match. Use `ui.ctx()` wherever raw `&egui::Context` access is needed — e.g. `EditorScreen::new(ui.ctx(), ...)` in later tasks.)

- [ ] **Step 7: Manually verify the window appears and the hotkey fires**

Run: `cargo run -p app`
Expected: a borderless, resizable window appears with the label text. Drag its edges to resize it, move it around. Press F9 and confirm `F9 event: Pressed` (and `Released`) print in the terminal. Close the window (e.g. `Alt+F4` or kill the process) with no panic.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/app
git commit -m "feat(app): add selection overlay window and F9 hotkey registration"
```

---

### Task 6: `app` — wire hotkey to capture start/stop (Selecting → Recording → Editing)

**Files:**
- Modify: `crates/app/src/main.rs`

**Interfaces:**
- Consumes: `selection::region_from_window_rect`, `capture::start_capture`, `editor::{Frame, FrameList}`

- [ ] **Step 1: Replace `main.rs`'s `App` with a state machine**

```rust
mod selection;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;

use eframe::egui;
use editor::{Frame, FrameList};
use global_hotkey::{
    hotkey::{Code, HotKey},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};

const CAPTURE_FPS: u32 = 10;

enum AppState {
    Selecting,
    Recording {
        stop_flag: Arc<AtomicBool>,
        handle: JoinHandle<Result<(), capture::CaptureError>>,
        rx: Receiver<Frame>,
    },
    Editing {
        frames: FrameList,
    },
}

struct App {
    _hotkey_manager: GlobalHotKeyManager,
    toggle_hotkey: HotKey,
    state: AppState,
    /// Set when a background operation (capture, export) fails partway
    /// through, so the current screen can show a warning without losing
    /// whatever work was already done. Cleared when the user starts a new
    /// attempt.
    last_error: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        let manager = GlobalHotKeyManager::new().expect("failed to create global hotkey manager");
        let toggle_hotkey = HotKey::new(None, Code::F9);
        manager
            .register(toggle_hotkey)
            .expect("failed to register F9 hotkey (is another app using it?)");
        Self { _hotkey_manager: manager, toggle_hotkey, state: AppState::Selecting, last_error: None }
    }
}

impl App {
    fn handle_toggle(&mut self, ctx: &egui::Context) {
        self.state = match std::mem::replace(&mut self.state, AppState::Selecting) {
            AppState::Selecting => {
                self.last_error = None;
                let rect = ctx
                    .input(|i| i.viewport().inner_rect)
                    .expect("window position is unavailable on this platform");
                let region = selection::region_from_window_rect(
                    rect.min.x,
                    rect.min.y,
                    rect.width(),
                    rect.height(),
                );
                let (tx, rx) = channel();
                let stop_flag = Arc::new(AtomicBool::new(false));
                let handle = capture::start_capture(region, CAPTURE_FPS, tx, stop_flag.clone());
                AppState::Recording { stop_flag, handle, rx }
            }
            AppState::Recording { stop_flag, handle, rx } => {
                stop_flag.store(true, Ordering::Relaxed);
                // A capture error only stops future frames — everything already
                // sent through `rx` is still collected below, so a mid-recording
                // failure never discards frames the user already captured.
                if let Err(e) = handle.join().expect("capture thread panicked") {
                    self.last_error = Some(format!("A gravação parou antes do esperado: {e}"));
                }
                let frames: Vec<Frame> = rx.try_iter().collect();
                AppState::Editing { frames: FrameList::new(frames) }
            }
            other @ AppState::Editing { .. } => other,
        };
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.id == self.toggle_hotkey.id() && event.state == global_hotkey::HotKeyState::Pressed {
                self.handle_toggle(&ctx);
            }
        }
        ctx.request_repaint();

        egui::CentralPanel::default().show(ui, |ui| match &self.state {
            AppState::Selecting => {
                ui.centered_and_justified(|ui| {
                    ui.label("Posicione esta janela sobre a região a gravar. F9 inicia a gravação.");
                });
            }
            AppState::Recording { .. } => {
                ui.centered_and_justified(|ui| {
                    ui.label("Gravando... F9 para parar.");
                });
            }
            AppState::Editing { frames } => {
                ui.label(format!("Gravação concluída: {} frames capturados.", frames.len()));
                if let Some(msg) = &self.last_error {
                    ui.colored_label(egui::Color32::RED, msg);
                }
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_resizable(true)
            .with_transparent(true)
            .with_inner_size([320.0, 240.0]),
        ..Default::default()
    };

    eframe::run_native("ScreenToGif", options, Box::new(|_cc| Ok(Box::new(App::default()))))
}
```

- [ ] **Step 2: Manually verify the full record cycle**

Run: `cargo run -p app`
Steps: resize/move the window over a visible part of the desktop (e.g. a terminal with some text you can change). Press F9 — label changes to "Gravando... F9 para parar." Wait ~2 seconds, change something visible in the region, press F9 again.
Expected: label changes to "Gravação concluída: N frames capturados." where N is roughly `2 * CAPTURE_FPS` (about 15-25, allowing scheduling variance). No panic.

- [ ] **Step 3: Commit**

```bash
git add crates/app
git commit -m "feat(app): wire F9 hotkey to capture start/stop state machine"
```

---

### Task 7: `app` — editor screen (thumbnail strip, delete, reorder)

**Files:**
- Create: `crates/app/src/editor_screen.rs`
- Modify: `crates/app/src/main.rs`

**Interfaces:**
- Produces: `pub fn selection_after_delete(selected: usize, deleted: usize, remaining_len: usize) -> usize` (pure, tested)
- Produces: `pub struct EditorScreen` with `new(&egui::Context, &FrameList) -> Self`, `show(&mut self, &mut egui::Ui, &FrameList) -> Option<EditorAction>`, `apply_delete(&mut self, usize)`, `apply_reorder(&mut self, usize, usize)`
- Produces: `pub enum EditorAction { Delete(usize), Reorder(usize, usize) }`

- [ ] **Step 1: Write the pure `selection_after_delete` helper with failing tests**

Create `crates/app/src/editor_screen.rs`:

```rust
/// Computes which thumbnail should stay selected after deleting the frame
/// at `deleted`, given the previously `selected` index and the list's
/// `remaining_len` after the deletion.
pub fn selection_after_delete(selected: usize, deleted: usize, remaining_len: usize) -> usize {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selecting_stays_put_when_deleting_a_later_frame() {
        assert_eq!(selection_after_delete(0, 2, 4), 0);
    }

    #[test]
    fn selection_shifts_left_when_deleting_an_earlier_frame() {
        assert_eq!(selection_after_delete(2, 0, 4), 1);
    }

    #[test]
    fn selection_clamps_when_deleting_the_last_frame() {
        assert_eq!(selection_after_delete(3, 3, 3), 2);
    }

    #[test]
    fn selection_is_zero_when_list_becomes_empty() {
        assert_eq!(selection_after_delete(0, 0, 0), 0);
    }
}
```

- [ ] **Step 2: Run the tests, verify they fail**

Run: `cargo test -p app selection_after_delete`
Expected: all 4 tests FAIL (panic: `not implemented`).

- [ ] **Step 3: Implement `selection_after_delete`**

```rust
pub fn selection_after_delete(selected: usize, deleted: usize, remaining_len: usize) -> usize {
    if remaining_len == 0 {
        0
    } else if deleted < selected {
        selected - 1
    } else {
        selected.min(remaining_len - 1)
    }
}
```

- [ ] **Step 4: Run the tests, verify they pass**

Run: `cargo test -p app selection_after_delete`
Expected: `test result: ok. 4 passed; 0 failed`

- [ ] **Step 5: Add `EditorScreen` to `editor_screen.rs`**

```rust
use eframe::egui;
use editor::FrameList;

pub enum EditorAction {
    Delete(usize),
    Reorder(usize, usize),
}

pub struct EditorScreen {
    textures: Vec<egui::TextureHandle>,
    pub selected: usize,
}

impl EditorScreen {
    pub fn new(ctx: &egui::Context, frames: &FrameList) -> Self {
        let textures = frames
            .frames()
            .iter()
            .enumerate()
            .map(|(i, frame)| {
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [frame.image.width() as usize, frame.image.height() as usize],
                    frame.image.as_raw(),
                );
                ctx.load_texture(format!("frame-{i}"), color_image, egui::TextureOptions::default())
            })
            .collect();
        Self { textures, selected: 0 }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, frames: &FrameList) -> Option<EditorAction> {
        let mut action = None;

        ui.horizontal(|ui| {
            for (i, texture) in self.textures.iter().enumerate() {
                let response = ui.add(egui::ImageButton::new(texture).selected(i == self.selected));
                if response.clicked() {
                    self.selected = i;
                }
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Excluir frame").clicked() && !frames.is_empty() {
                action = Some(EditorAction::Delete(self.selected));
            }
            if ui.button("Mover para a esquerda").clicked() && self.selected > 0 {
                action = Some(EditorAction::Reorder(self.selected, self.selected - 1));
            }
            if ui.button("Mover para a direita").clicked() && self.selected + 1 < frames.len() {
                action = Some(EditorAction::Reorder(self.selected, self.selected + 1));
            }
        });

        if let Some(texture) = self.textures.get(self.selected) {
            ui.image(texture);
        }

        action
    }

    pub fn apply_delete(&mut self, index: usize) {
        self.textures.remove(index);
        self.selected = selection_after_delete(self.selected, index, self.textures.len());
    }

    pub fn apply_reorder(&mut self, from: usize, to: usize) {
        let texture = self.textures.remove(from);
        self.textures.insert(to, texture);
        self.selected = to;
    }
}
```

- [ ] **Step 6: Wire `EditorScreen` into `main.rs`**

Add `mod editor_screen;` at the top, add `use editor_screen::{EditorAction, EditorScreen};`, change the `Editing` variant and its construction/rendering:

```rust
enum AppState {
    Selecting,
    Recording {
        stop_flag: Arc<AtomicBool>,
        handle: JoinHandle<Result<(), capture::CaptureError>>,
        rx: Receiver<Frame>,
    },
    Editing {
        frames: FrameList,
        screen: EditorScreen,
    },
}
```

In `handle_toggle`, replace the `Recording { .. }` arm's final line:

```rust
AppState::Recording { stop_flag, handle, rx } => {
    stop_flag.store(true, Ordering::Relaxed);
    handle.join().expect("capture thread panicked").expect("capture failed");
    let frames = FrameList::new(rx.try_iter().collect());
    let screen = EditorScreen::new(ctx, &frames);
    AppState::Editing { frames, screen }
}
```

In `update`, replace the `Editing` render arm:

```rust
AppState::Editing { frames, screen } => {
    ui.label(format!("Gravação concluída: {} frames capturados.", frames.len()));
    if let Some(msg) = &self.last_error {
        ui.colored_label(egui::Color32::RED, msg);
    }
    if let Some(action) = screen.show(ui, frames) {
        match action {
            EditorAction::Delete(i) => {
                frames.delete(i).expect("index came from the UI, must be valid");
                screen.apply_delete(i);
            }
            EditorAction::Reorder(from, to) => {
                frames.reorder(from, to).expect("index came from the UI, must be valid");
                screen.apply_reorder(from, to);
            }
        }
    }
}
```

(Note: this match arm now needs `&mut self.state`, so change `egui::CentralPanel::default().show(ctx, |ui| match &self.state {` to `match &mut self.state {`, and update the `Selecting`/`Recording` arms to bind by shared reference where they don't mutate, e.g. `AppState::Selecting => { ... }` stays the same since egui closures can take a `&mut` outer match and still just read fields. `self.last_error` is read here via Rust 2021's per-field closure capture, which is disjoint from the `&mut self.state` capture used for the match.)

- [ ] **Step 7: Manually verify editing**

Run: `cargo run -p app`, record ~2 seconds of a region with visible movement (e.g. move the mouse or change terminal text).
Expected: in the editor screen, thumbnails appear for every captured frame; clicking a thumbnail selects it and shows it enlarged below; "Excluir frame" removes the selected thumbnail and the frame count label decreases; "Mover para a esquerda"/"Mover para a direita" reorder thumbnails visibly. No panic when deleting down to a single frame.

- [ ] **Step 8: Commit**

```bash
git add crates/app
git commit -m "feat(app): add editor screen with thumbnail delete/reorder"
```

---

### Task 8: `app` — crop tool

**Files:**
- Create: `crates/app/src/crop_tool.rs`
- Modify: `crates/app/src/editor_screen.rs`
- Modify: `crates/app/src/main.rs`

**Interfaces:**
- Produces: `pub fn crop_rect_from_drag(drag_start: (f32, f32), drag_end: (f32, f32), displayed_size: (f32, f32), image_pixel_size: (u32, u32)) -> editor::CropRect`
- Produces: `EditorAction::Crop(editor::CropRect)` (new variant)

- [ ] **Step 1: Write `crop_tool.rs` with a stub and failing tests**

```rust
use editor::CropRect;

/// Converts a drag rectangle drawn over a displayed image back into pixel
/// coordinates of the original image, accounting for the display scale
/// (the image widget may render the image larger or smaller than its
/// actual pixel dimensions).
pub fn crop_rect_from_drag(
    drag_start: (f32, f32),
    drag_end: (f32, f32),
    displayed_size: (f32, f32),
    image_pixel_size: (u32, u32),
) -> CropRect {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_drag_at_1to1_scale() {
        let rect = crop_rect_from_drag((10.0, 20.0), (50.0, 60.0), (100.0, 100.0), (100, 100));
        assert_eq!((rect.x, rect.y, rect.width, rect.height), (10, 20, 40, 40));
    }

    #[test]
    fn scales_drag_when_displayed_smaller_than_actual() {
        // Image is 200x200 pixels but displayed at 100x100 (half scale).
        let rect = crop_rect_from_drag((10.0, 10.0), (30.0, 30.0), (100.0, 100.0), (200, 200));
        assert_eq!((rect.x, rect.y, rect.width, rect.height), (20, 20, 40, 40));
    }

    #[test]
    fn handles_drag_in_reverse_direction() {
        let rect = crop_rect_from_drag((50.0, 60.0), (10.0, 20.0), (100.0, 100.0), (100, 100));
        assert_eq!((rect.x, rect.y, rect.width, rect.height), (10, 20, 40, 40));
    }

    #[test]
    fn clamps_to_image_bounds() {
        let rect = crop_rect_from_drag((90.0, 90.0), (150.0, 150.0), (100.0, 100.0), (100, 100));
        assert_eq!((rect.x, rect.y), (90, 90));
        assert_eq!((rect.width, rect.height), (10, 10));
    }
}
```

- [ ] **Step 2: Run the tests, verify they fail**

Run: `cargo test -p app crop_rect_from_drag`
Expected: all 4 tests FAIL (panic: `not implemented`).

- [ ] **Step 3: Implement `crop_rect_from_drag`**

```rust
pub fn crop_rect_from_drag(
    drag_start: (f32, f32),
    drag_end: (f32, f32),
    displayed_size: (f32, f32),
    image_pixel_size: (u32, u32),
) -> CropRect {
    let scale_x = image_pixel_size.0 as f32 / displayed_size.0;
    let scale_y = image_pixel_size.1 as f32 / displayed_size.1;

    let (x0, x1) = (drag_start.0.min(drag_end.0), drag_start.0.max(drag_end.0));
    let (y0, y1) = (drag_start.1.min(drag_end.1), drag_start.1.max(drag_end.1));

    let px_x = (x0 * scale_x).round().max(0.0) as u32;
    let px_y = (y0 * scale_y).round().max(0.0) as u32;
    let px_x1 = ((x1 * scale_x).round() as u32).min(image_pixel_size.0);
    let px_y1 = ((y1 * scale_y).round() as u32).min(image_pixel_size.1);

    CropRect {
        x: px_x,
        y: px_y,
        width: px_x1.saturating_sub(px_x).max(1),
        height: px_y1.saturating_sub(px_y).max(1),
    }
}
```

- [ ] **Step 4: Run the tests, verify they pass**

Run: `cargo test -p app crop_rect_from_drag`
Expected: `test result: ok. 4 passed; 0 failed`

- [ ] **Step 5: Add crop dragging to `editor_screen.rs`**

Add `mod crop_tool;` to `main.rs`. In `editor_screen.rs`, add a `cropping: bool` field (default `false`) and a `drag_start: Option<egui::Pos2>` field to `EditorScreen`, plus a "Cortar" toggle button next to the existing buttons in `show`:

```rust
if ui.button(if self.cropping { "Cancelar corte" } else { "Cortar" }).clicked() {
    self.cropping = !self.cropping;
    self.drag_start = None;
}
```

Replace the final `if let Some(texture) = self.textures.get(self.selected) { ui.image(texture); }` block with drag handling when `self.cropping` is true:

```rust
if let Some(texture) = self.textures.get(self.selected) {
    let image_response = ui.add(egui::Image::new(texture).sense(egui::Sense::drag()));
    if self.cropping {
        if image_response.drag_started() {
            self.drag_start = ui.input(|i| i.pointer.interact_pos());
        }
        if let (Some(start), Some(current)) = (self.drag_start, ui.input(|i| i.pointer.interact_pos())) {
            let rect_on_screen = egui::Rect::from_two_pos(start, current);
            ui.painter().rect_stroke(rect_on_screen, 0.0, egui::Stroke::new(2.0, egui::Color32::YELLOW));
        }
        if image_response.drag_stopped() {
            if let (Some(start), Some(end)) = (self.drag_start, ui.input(|i| i.pointer.interact_pos())) {
                let rect = image_response.rect;
                let to_local = |p: egui::Pos2| (p.x - rect.min.x, p.y - rect.min.y);
                let frame = &frames.frames()[self.selected];
                action = Some(EditorAction::Crop(crate::crop_tool::crop_rect_from_drag(
                    to_local(start),
                    to_local(end),
                    (rect.width(), rect.height()),
                    (frame.image.width(), frame.image.height()),
                )));
                self.cropping = false;
                self.drag_start = None;
            }
        }
    }
}
```

Add `EditorAction::Crop(editor::CropRect)` to the `EditorAction` enum.

- [ ] **Step 6: Handle the crop action in `main.rs`**

In the `Editing` render arm's `match action`, add:

```rust
EditorAction::Crop(rect) => {
    frames.crop(rect).expect("crop rect came from the UI, must be valid");
    *screen = EditorScreen::new(ctx, frames); // frame dimensions changed, rebuild all textures
}
```

- [ ] **Step 7: Manually verify cropping**

Run: `cargo run -p app`, record ~2 seconds, go to the editor, click "Cortar", drag a rectangle over the enlarged preview, release.
Expected: the preview and every thumbnail shrink to the dragged region; frame count stays the same; dragging in any direction (including bottom-right to top-left) produces the same crop.

- [ ] **Step 8: Commit**

```bash
git add crates/app
git commit -m "feat(app): add crop tool to editor screen"
```

---

### Task 9: `app` — export screen (gifski progress + save dialog)

**Files:**
- Create: `crates/app/src/export_screen.rs`
- Modify: `crates/app/src/main.rs`

**Interfaces:**
- Produces: `pub struct ExportJob { pub progress: Arc<Mutex<(usize, usize)>>, pub handle: JoinHandle<Result<(), encoder::EncodeError>> }`
- Produces: `pub fn start_export(frames: &FrameList, output_path: PathBuf) -> ExportJob`

- [ ] **Step 1: Write `export_screen.rs` with a stub and a failing test**

```rust
use editor::{Frame, FrameList};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub struct ExportJob {
    pub progress: Arc<Mutex<(usize, usize)>>,
    pub handle: JoinHandle<Result<(), encoder::EncodeError>>,
}

pub fn start_export(frames: &FrameList, output_path: PathBuf) -> ExportJob {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn start_export_produces_a_gif_file_and_reports_full_progress() {
        let frames = FrameList::new(vec![
            Frame { image: RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255])), timestamp_ms: 0 },
            Frame { image: RgbaImage::from_pixel(2, 2, image::Rgba([0, 255, 0, 255])), timestamp_ms: 100 },
        ]);
        let output = std::env::temp_dir().join("export_screen_test.gif");

        let job = start_export(&frames, output.clone());
        job.handle.join().expect("export thread panicked").expect("export failed");

        let (current, total) = *job.progress.lock().unwrap();
        assert_eq!((current, total), (2, 2));
        assert!(output.exists());

        std::fs::remove_file(&output).ok();
    }
}
```

- [ ] **Step 2: Run the test, verify it fails**

Run: `cargo test -p app start_export`
Expected: FAIL (panic: `not implemented`).

- [ ] **Step 3: Implement `start_export`**

```rust
pub fn start_export(frames: &FrameList, output_path: PathBuf) -> ExportJob {
    let progress = Arc::new(Mutex::new((0usize, frames.len())));
    let progress_for_thread = progress.clone();
    let owned_frames: Vec<Frame> = frames.frames().to_vec();

    let handle = std::thread::spawn(move || {
        encoder::encode_gif(&owned_frames, output_path, move |current, total| {
            *progress_for_thread.lock().unwrap() = (current, total);
        })
    });

    ExportJob { progress, handle }
}
```

- [ ] **Step 4: Run the test, verify it passes**

Run: `cargo test -p app start_export`
Expected: `test result: ok. 1 passed; 0 failed`

- [ ] **Step 5: Wire export into `main.rs`**

Add `mod export_screen;` and `use export_screen::{start_export, ExportJob};` and `use std::path::PathBuf;` and `use std::sync::Mutex;`. Extend `AppState`:

```rust
enum AppState {
    Selecting,
    Recording {
        stop_flag: Arc<AtomicBool>,
        handle: JoinHandle<Result<(), capture::CaptureError>>,
        rx: Receiver<Frame>,
    },
    Editing {
        frames: FrameList,
        screen: EditorScreen,
    },
    Exporting {
        frames: FrameList,
        screen: EditorScreen,
        job: ExportJob,
        output_path: PathBuf,
    },
    Done {
        output_path: PathBuf,
    },
}
```

`App` already has `last_error: Option<String>` (added in Task 6) — this task reuses it for export failures.

In the `Editing` render arm, add an "Exportar" button that opens the save dialog and transitions state:

```rust
AppState::Editing { frames, screen } => {
    ui.label(format!("Gravação concluída: {} frames capturados.", frames.len()));
    if let Some(msg) = &self.last_error {
        ui.colored_label(egui::Color32::RED, msg);
    }
    if let Some(action) = screen.show(ui, frames) {
        match action {
            EditorAction::Delete(i) => {
                frames.delete(i).expect("index came from the UI, must be valid");
                screen.apply_delete(i);
            }
            EditorAction::Reorder(from, to) => {
                frames.reorder(from, to).expect("index came from the UI, must be valid");
                screen.apply_reorder(from, to);
            }
            EditorAction::Crop(rect) => {
                frames.crop(rect).expect("crop rect came from the UI, must be valid");
                *screen = EditorScreen::new(ctx, frames);
            }
        }
    }
    if ui.button("Exportar").clicked() {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("GIF", &["gif"])
            .set_file_name("recording.gif")
            .save_file()
        {
            self.last_error = None;
            should_start_export = Some(path);
        }
    }
}
```

Because match arms borrow `self.state` and we also need to mutate `self.state` to start the export, capture the chosen path into a local `let mut should_start_export: Option<PathBuf> = None;` declared before the `match &mut self.state { ... }` block, and after the block:

```rust
if let Some(path) = should_start_export {
    self.state = match std::mem::replace(&mut self.state, AppState::Selecting) {
        AppState::Editing { frames, screen } => {
            let job = start_export(&frames, path.clone());
            AppState::Exporting { frames, screen, job, output_path: path }
        }
        other => other,
    };
}
```

Add render + completion-check arms:

```rust
AppState::Exporting { job, .. } => {
    let (current, total) = *job.progress.lock().unwrap();
    ui.label("Exportando...");
    ui.add(egui::ProgressBar::new(if total == 0 { 0.0 } else { current as f32 / total as f32 }));
}
AppState::Done { output_path } => {
    ui.label(format!("Salvo em: {}", output_path.display()));
}
```

And, right after the `egui::CentralPanel::default().show(...)` call in `update`, check whether an in-progress export has finished and transition out of it:

```rust
if let AppState::Exporting { job, .. } = &self.state {
    if job.handle.is_finished() {
        self.state = match std::mem::replace(&mut self.state, AppState::Selecting) {
            AppState::Exporting { frames, screen, job, output_path } => {
                match job.handle.join().expect("export thread panicked") {
                    Ok(()) => AppState::Done { output_path },
                    Err(e) => {
                        self.last_error = Some(format!("Falha ao exportar: {e}"));
                        AppState::Editing { frames, screen }
                    }
                }
            }
            other => other,
        };
    }
}
```

- [ ] **Step 6: Manually verify the export flow**

Run: `cargo run -p app`, record ~2 seconds, in the editor click "Exportar", choose a save location (e.g. `/tmp/test.gif`).
Expected: progress bar advances and completes, screen switches to "Salvo em: /tmp/test.gif". Open the file (e.g. `xdg-open /tmp/test.gif`) and confirm it plays back the recorded region as an animated GIF.

- [ ] **Step 7: Commit**

```bash
git add crates/app
git commit -m "feat(app): add export screen with gifski progress and save dialog"
```

---

### Task 10: README and end-to-end checklist

**Files:**
- Create: `README.md`

- [ ] **Step 1: Write `README.md`**

```markdown
# ScreenToGif (Rust, Linux/X11 MVP)

Grava uma região da tela, permite editar os frames (excluir, reordenar, cortar) e exporta um GIF.

## Requisitos

- Linux com sessão X11 (rode `echo $XDG_SESSION_TYPE` para confirmar — deve imprimir `x11`).
- Rust estável (`rustup show` para conferir).

## Build

    cargo build --workspace

## Rodar

    cargo run -p app

## Fluxo de uso

1. Uma janela sem bordas aparece — arraste/redimensione para cobrir a região que você quer gravar.
2. Pressione **F9** para iniciar a gravação.
3. Pressione **F9** de novo para parar. A tela muda para o editor.
4. No editor: clique numa miniatura para selecioná-la, **Excluir frame** remove, **Mover para a esquerda/direita** reordena, **Cortar** + arrastar sobre o preview recorta todos os frames.
5. Clique em **Exportar**, escolha onde salvar; acompanhe a barra de progresso até "Salvo em: ...".

## Escopo desta versão (MVP)

Ver `docs/superpowers/specs/2026-08-12-screentogif-rust-linux-design.md` para o design completo. Fora de escopo por enquanto: Wayland, webcam, modo board, filtros de imagem, exportação para vídeo/APNG/PSD, salvar/carregar projeto.

## Testes automatizados

    cargo test --workspace

`capture` não tem testes automatizados (depende de um display X11 real) — veja `crates/capture/examples/manual_capture.rs` para verificação manual.
```

- [ ] **Step 2: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: all tests across `editor`, `encoder`, and `app` pass (no test target in `capture`, that's expected).

- [ ] **Step 3: Run the full manual end-to-end checklist once**

Run: `cargo run -p app` and walk through: select region → F9 start → change something visible → F9 stop → delete a frame → reorder two frames → crop → export to a temp path → open the resulting GIF and confirm it plays and reflects the edits (fewer frames, reordered content, cropped bounds).
Expected: every step behaves as described with no crash.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: add README with build/run instructions and MVP scope"
```
