# Capture Flow & Editor Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current "the app window is the capture region" MVP flow with a real Project screen (FPS presets, Tela Inteira / Selecionar Área), a full-screen selection overlay, an always-on-top recording indicator, and a video-editor-style Editing screen (large preview, tool sidebar, filmstrip) with four tools: Selecionar, Recortar, Blur, Texto, plus a Prévia (play) button.

**Architecture:** Three coordinated `egui`/`eframe` viewports (main window, selection overlay, recording indicator) driving a restructured `AppState` machine in the `app` crate; new pure operations (`duplicate`, `blur`, `add_text`) added to `FrameList` in the `editor` crate; the `capture` crate gains small monitor-geometry helpers. No changes to the GIF-only export path.

**Tech Stack:** Rust, `egui`/`eframe` 0.36.1 (multi-viewport API, new usage), `xcap` (already a dependency via `capture`), `imageproc` + `ab_glyph` (new, for text rendering), `image::imageops::blur` (already available via the existing `image` dependency).

**Spec:** `docs/superpowers/specs/2026-08-13-screentogif-capture-editor-redesign-design.md`

## Global Constraints

- Target platform: Linux/X11 only (Wayland out of scope) — unchanged from the original MVP.
- Single-session flow: "← Nova gravação" discards the in-memory session with no confirmation dialog.
- Export format stays GIF-only via `gifski` — `export_screen.rs` is unchanged by this plan.
- No per-frame delay/timing editing — frames keep using their captured `timestamp_ms`.
- No drag-and-drop reordering in the filmstrip — reorder stays ◀/▶ button-driven.
- No window-picker capture mode and no monitor picker for "Tela Inteira" — it captures the monitor under the main window's current position.
- No project save/load to disk.
- Text and blur apply to **every** frame at a fixed region/position (same model as `crop`), not per-individual-frame.
- Prefer `cargo add <crate>` over hand-picked version numbers in `Cargo.toml`.
- F9 is a global **stop-recording-only** shortcut, active only while `AppState::Recording` is current; it has no effect in any other state.

---

## File Structure

```
crates/
  editor/
    Cargo.toml                    # + imageproc, ab_glyph
    assets/
      DejaVuSans.ttf               # new: embedded font for add_text
      DejaVuSans-LICENSE.txt       # new: font's copyright/license text
    src/lib.rs                    # + duplicate, blur, add_text, EditorError::EmptyText
  capture/
    src/lib.rs                    # + Region derives, bounding_box, monitor_bounds_at, virtual_screen_bounds
  app/
    src/main.rs                   # AppState machine rewritten (Project/SelectingArea/Recording/Editing/...)
    src/project_screen.rs         # new: "Gravar tela" screen (FPS presets, Tela Inteira button, later Selecionar Área)
    src/selection_overlay.rs      # new: region_from_drag (pure) + full-desktop overlay viewport
    src/recording_indicator.rs    # new: always-on-top REC indicator viewport
    src/text_tool.rs              # new: text_position_from_click (pure)
    src/editor_screen.rs          # rewritten: two-column layout (preview + tool sidebar) + filmstrip
    src/crop_tool.rs              # unchanged: crop_rect_from_drag reused for Recortar and Blur
    src/export_screen.rs          # unchanged
    src/selection.rs              # deleted: superseded by selection_overlay.rs
README.md                         # rewritten manual checklist
```

---

### Task 1: `editor` crate — duplicate

**Files:**
- Modify: `crates/editor/src/lib.rs`

**Interfaces:**
- Produces: `FrameList::duplicate(&mut self, index: usize) -> Result<(), EditorError>`

- [ ] **Step 1: Add the failing tests**

Add to the `tests` module in `crates/editor/src/lib.rs` (it already has `make_frame`/`tags` helpers from the existing `delete`/`reorder` tests — reuse them):

```rust
#[test]
fn duplicate_inserts_a_copy_right_after_the_source() {
    let mut list = FrameList::new(vec![make_frame(1), make_frame(2), make_frame(3)]);
    list.duplicate(0).unwrap();
    assert_eq!(tags(&list), vec![1, 1, 2, 3]);
    assert_eq!(list.len(), 4);
}

#[test]
fn duplicate_out_of_bounds_returns_error() {
    let mut list = FrameList::new(vec![make_frame(1)]);
    assert_eq!(list.duplicate(5), Err(EditorError::IndexOutOfBounds));
}
```

- [ ] **Step 2: Add the stub method**

Add to `impl FrameList`:

```rust
pub fn duplicate(&mut self, index: usize) -> Result<(), EditorError> {
    unimplemented!()
}
```

- [ ] **Step 3: Run the tests, verify the two new ones fail**

Run: `cargo test -p editor duplicate`
Expected: 2 tests FAIL (panic: `not implemented`).

- [ ] **Step 4: Implement `duplicate`**

```rust
pub fn duplicate(&mut self, index: usize) -> Result<(), EditorError> {
    if index >= self.frames.len() {
        return Err(EditorError::IndexOutOfBounds);
    }
    let clone = self.frames[index].clone();
    self.frames.insert(index + 1, clone);
    Ok(())
}
```

- [ ] **Step 5: Run the tests, verify they pass**

Run: `cargo test -p editor`
Expected: all tests pass (11 total: the existing 9 plus these 2).

- [ ] **Step 6: Commit**

```bash
git add crates/editor/src/lib.rs
git commit -m "feat(editor): add duplicate operation to FrameList"
```

---

### Task 2: `editor` crate — blur

**Files:**
- Modify: `crates/editor/src/lib.rs`

**Interfaces:**
- Consumes: `image::imageops::{crop_imm, blur, overlay}` (already available — `image` is an existing dependency, no `Cargo.toml` change needed)
- Produces: `FrameList::blur(&mut self, rect: CropRect, sigma: f32) -> Result<(), EditorError>`

- [ ] **Step 1: Add the failing tests**

Add to the `tests` module:

```rust
#[test]
fn blur_smooths_pixels_only_inside_the_region_across_all_frames() {
    let make = || {
        RgbaImage::from_fn(4, 4, |x, _y| {
            if x < 2 { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([255, 255, 255, 255]) }
        })
    };
    let mut list = FrameList::new(vec![
        Frame { image: make(), timestamp_ms: 0 },
        Frame { image: make(), timestamp_ms: 100 },
    ]);
    list.blur(CropRect { x: 1, y: 0, width: 2, height: 4 }, 2.0).unwrap();
    for frame in list.frames() {
        assert_eq!(frame.image.get_pixel(0, 0), &image::Rgba([0, 0, 0, 255]));
        assert_eq!(frame.image.get_pixel(3, 0), &image::Rgba([255, 255, 255, 255]));
        let mixed = frame.image.get_pixel(1, 0);
        assert_ne!(mixed, &image::Rgba([0, 0, 0, 255]));
        assert_ne!(mixed, &image::Rgba([255, 255, 255, 255]));
    }
}

#[test]
fn blur_rect_outside_frame_bounds_returns_error() {
    let mut list = FrameList::new(vec![Frame {
        image: RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255])),
        timestamp_ms: 0,
    }]);
    let err = list.blur(CropRect { x: 3, y: 3, width: 4, height: 4 }, 2.0).unwrap_err();
    assert_eq!(err, EditorError::InvalidCropRect);
}

#[test]
fn blur_with_zero_size_rect_returns_error() {
    let mut list = FrameList::new(vec![Frame {
        image: RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255])),
        timestamp_ms: 0,
    }]);
    let err = list.blur(CropRect { x: 0, y: 0, width: 0, height: 2 }, 2.0).unwrap_err();
    assert_eq!(err, EditorError::InvalidCropRect);
}
```

(`blur_smooths_pixels_only_inside_the_region_across_all_frames` is empirically verified: `image::imageops::blur` with `sigma = 2.0` on a 2px-wide hard black/white edge produces `(102,102,102)` and `(153,153,153)` at the two columns — both far from the pure `0`/`255` inputs.)

- [ ] **Step 2: Add the stub method**

Add to `impl FrameList`:

```rust
pub fn blur(&mut self, rect: CropRect, sigma: f32) -> Result<(), EditorError> {
    unimplemented!()
}
```

- [ ] **Step 3: Run the tests, verify the three new ones fail**

Run: `cargo test -p editor blur`
Expected: 3 tests FAIL (panic: `not implemented`).

- [ ] **Step 4: Implement `blur`**

```rust
pub fn blur(&mut self, rect: CropRect, sigma: f32) -> Result<(), EditorError> {
    if rect.width == 0 || rect.height == 0 {
        return Err(EditorError::InvalidCropRect);
    }
    for frame in &self.frames {
        if rect.x + rect.width > frame.image.width() || rect.y + rect.height > frame.image.height() {
            return Err(EditorError::InvalidCropRect);
        }
    }
    for frame in &mut self.frames {
        let region = image::imageops::crop_imm(&frame.image, rect.x, rect.y, rect.width, rect.height).to_image();
        let blurred = image::imageops::blur(&region, sigma);
        image::imageops::overlay(&mut frame.image, &blurred, rect.x as i64, rect.y as i64);
    }
    Ok(())
}
```

- [ ] **Step 5: Run the tests, verify they pass**

Run: `cargo test -p editor`
Expected: all tests pass (14 total).

- [ ] **Step 6: Commit**

```bash
git add crates/editor/src/lib.rs
git commit -m "feat(editor): add blur operation to FrameList"
```

---

### Task 3: `editor` crate — add_text

**Files:**
- Modify: `crates/editor/Cargo.toml`
- Modify: `crates/editor/src/lib.rs`
- Create: `crates/editor/assets/DejaVuSans.ttf`
- Create: `crates/editor/assets/DejaVuSans-LICENSE.txt`

**Interfaces:**
- Consumes: `imageproc::drawing::draw_text_mut`, `ab_glyph::{FontRef, PxScale}` (new dependencies)
- Produces: `EditorError::EmptyText` (new variant), `FrameList::add_text(&mut self, position: (u32, u32), text: String, font_size: f32, color: [u8; 4]) -> Result<(), EditorError>`

- [ ] **Step 1: Add the dependencies**

```bash
cd crates/editor
cargo add imageproc ab_glyph
cd ../..
```

- [ ] **Step 2: Vendor the font**

```bash
mkdir -p crates/editor/assets
cp /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf crates/editor/assets/DejaVuSans.ttf
cp /usr/share/doc/fonts-dejavu-core/copyright crates/editor/assets/DejaVuSans-LICENSE.txt
```

If `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf` doesn't exist on the machine running this step, install it first (`apt install fonts-dejavu-core` on Debian/Ubuntu) or substitute any other permissively-licensed (OFL or similar) `.ttf` file, updating the license file accordingly — the font just needs to be legally redistributable as a binary asset in this repo.

- [ ] **Step 3: Add the failing tests and the `EmptyText` error variant**

Add `EmptyText` to the `EditorError` enum and its `Display` impl:

```rust
#[derive(Debug, PartialEq, Eq)]
pub enum EditorError {
    IndexOutOfBounds,
    InvalidCropRect,
    EmptyText,
}
```

```rust
impl std::fmt::Display for EditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditorError::IndexOutOfBounds => write!(f, "frame index out of bounds"),
            EditorError::InvalidCropRect => write!(f, "crop rect is invalid for this frame"),
            EditorError::EmptyText => write!(f, "text must not be empty"),
        }
    }
}
```

Add to the `tests` module:

```rust
#[test]
fn add_text_changes_pixels_in_its_area_across_all_frames() {
    let mut list = FrameList::new(vec![
        Frame { image: RgbaImage::from_pixel(60, 30, image::Rgba([0, 0, 0, 255])), timestamp_ms: 0 },
        Frame { image: RgbaImage::from_pixel(60, 30, image::Rgba([0, 0, 0, 255])), timestamp_ms: 100 },
    ]);
    list.add_text((5, 2), "Hi".to_string(), 20.0, [255, 255, 255, 255]).unwrap();
    for frame in list.frames() {
        let changed = frame
            .image
            .pixels()
            .filter(|p| **p != image::Rgba([0, 0, 0, 255]))
            .count();
        assert!(changed > 0, "expected at least one pixel changed by the drawn text");
    }
}

#[test]
fn add_text_rejects_empty_text() {
    let mut list = FrameList::new(vec![Frame {
        image: RgbaImage::from_pixel(10, 10, image::Rgba([0, 0, 0, 255])),
        timestamp_ms: 0,
    }]);
    let err = list.add_text((0, 0), String::new(), 20.0, [255, 255, 255, 255]).unwrap_err();
    assert_eq!(err, EditorError::EmptyText);
}
```

(Empirically verified: `draw_text_mut` with the vendored `DejaVuSans.ttf`, drawing `"Hi"` at size `20.0` onto a solid-black 60×30 image, changes 127 pixels.)

- [ ] **Step 4: Add the stub method**

Add near the top of the file (module-level, alongside the other `use` statements):

```rust
use ab_glyph::{FontRef, PxScale};
use imageproc::drawing::draw_text_mut;

static FONT_BYTES: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");
```

Add to `impl FrameList`:

```rust
pub fn add_text(
    &mut self,
    position: (u32, u32),
    text: String,
    font_size: f32,
    color: [u8; 4],
) -> Result<(), EditorError> {
    unimplemented!()
}
```

- [ ] **Step 5: Run the tests, verify the two new ones fail**

Run: `cargo test -p editor add_text`
Expected: 2 tests FAIL (panic: `not implemented`).

- [ ] **Step 6: Implement `add_text`**

```rust
pub fn add_text(
    &mut self,
    position: (u32, u32),
    text: String,
    font_size: f32,
    color: [u8; 4],
) -> Result<(), EditorError> {
    if text.is_empty() {
        return Err(EditorError::EmptyText);
    }
    let font = FontRef::try_from_slice(FONT_BYTES).expect("bundled font must be valid");
    let scale = PxScale::from(font_size);
    let pixel = image::Rgba(color);
    for frame in &mut self.frames {
        draw_text_mut(&mut frame.image, pixel, position.0 as i32, position.1 as i32, scale, &font, &text);
    }
    Ok(())
}
```

- [ ] **Step 7: Run the tests, verify they pass**

Run: `cargo test -p editor`
Expected: all tests pass (16 total).

- [ ] **Step 8: Commit**

```bash
git add crates/editor/Cargo.toml crates/editor/Cargo.lock crates/editor/src/lib.rs crates/editor/assets
git commit -m "feat(editor): add add_text operation to FrameList"
```

---

### Task 4: `app` crate — new pure helpers (text position, region-from-drag, duplicate selection)

**Files:**
- Create: `crates/app/src/text_tool.rs`
- Create: `crates/app/src/selection_overlay.rs` (pure function only in this task — the overlay viewport itself is added in Task 8)
- Modify: `crates/app/src/editor_screen.rs` (the current, pre-redesign file — add `selection_after_duplicate` alongside the existing `selection_after_delete`; both survive into the Task 9 rewrite)
- Modify: `crates/app/src/main.rs` (add the two new `mod` declarations)

**Interfaces:**
- Produces: `text_tool::text_position_from_click(click: (f32, f32), displayed_size: (f32, f32), image_pixel_size: (u32, u32)) -> (u32, u32)`
- Produces: `selection_overlay::region_from_drag(viewport_origin: (f32, f32), drag_start: (f32, f32), drag_end: (f32, f32)) -> capture::Region`
- Produces: `editor_screen::selection_after_duplicate(duplicated_at: usize) -> usize`

- [ ] **Step 1: Write `text_tool.rs` with a stub and failing tests**

```rust
/// Converts a click on the displayed preview image into a pixel position
/// in the original image, accounting for display scale (mirrors
/// `crop_tool::crop_rect_from_drag`'s scale math for a single point).
pub fn text_position_from_click(
    click: (f32, f32),
    displayed_size: (f32, f32),
    image_pixel_size: (u32, u32),
) -> (u32, u32) {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_click_at_1to1_scale() {
        assert_eq!(text_position_from_click((10.0, 20.0), (100.0, 100.0), (100, 100)), (10, 20));
    }

    #[test]
    fn scales_click_when_displayed_smaller_than_actual() {
        assert_eq!(text_position_from_click((10.0, 10.0), (100.0, 100.0), (200, 200)), (20, 20));
    }

    #[test]
    fn clamps_to_image_bounds() {
        assert_eq!(text_position_from_click((150.0, 150.0), (100.0, 100.0), (100, 100)), (99, 99));
    }
}
```

- [ ] **Step 2: Run the tests, verify they fail**

Run: `cargo test -p app text_position_from_click`
Expected: 3 tests FAIL (panic: `not implemented`).

- [ ] **Step 3: Implement `text_position_from_click`**

```rust
pub fn text_position_from_click(
    click: (f32, f32),
    displayed_size: (f32, f32),
    image_pixel_size: (u32, u32),
) -> (u32, u32) {
    let scale_x = image_pixel_size.0 as f32 / displayed_size.0;
    let scale_y = image_pixel_size.1 as f32 / displayed_size.1;
    let px_x = (click.0 * scale_x).round().max(0.0) as u32;
    let px_y = (click.1 * scale_y).round().max(0.0) as u32;
    (
        px_x.min(image_pixel_size.0.saturating_sub(1)),
        px_y.min(image_pixel_size.1.saturating_sub(1)),
    )
}
```

- [ ] **Step 4: Run the tests, verify they pass**

Run: `cargo test -p app text_position_from_click`
Expected: `test result: ok. 3 passed; 0 failed`

- [ ] **Step 5: Write `selection_overlay.rs` with a stub and failing tests**

```rust
/// Converts a drag on the full-desktop selection overlay (whose own
/// viewport sits at `viewport_origin` in global desktop coordinates) into
/// a `capture::Region` in those same global coordinates.
pub fn region_from_drag(
    viewport_origin: (f32, f32),
    drag_start: (f32, f32),
    drag_end: (f32, f32),
) -> capture::Region {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_a_forward_drag_at_the_viewport_origin() {
        let region = region_from_drag((0.0, 0.0), (10.0, 20.0), (110.0, 220.0));
        assert_eq!((region.x, region.y, region.width, region.height), (10, 20, 100, 200));
    }

    #[test]
    fn offsets_by_the_viewport_origin() {
        let region = region_from_drag((1920.0, 0.0), (10.0, 20.0), (110.0, 220.0));
        assert_eq!((region.x, region.y, region.width, region.height), (1930, 20, 100, 200));
    }

    #[test]
    fn handles_a_reverse_drag() {
        let region = region_from_drag((0.0, 0.0), (110.0, 220.0), (10.0, 20.0));
        assert_eq!((region.x, region.y, region.width, region.height), (10, 20, 100, 200));
    }

    #[test]
    fn clamps_a_degenerate_drag_to_at_least_one_pixel() {
        let region = region_from_drag((0.0, 0.0), (10.0, 10.0), (10.0, 10.0));
        assert_eq!((region.width, region.height), (1, 1));
    }
}
```

- [ ] **Step 6: Run the tests, verify they fail**

Run: `cargo test -p app region_from_drag`
Expected: 4 tests FAIL (panic: `not implemented`).

- [ ] **Step 7: Implement `region_from_drag`**

```rust
pub fn region_from_drag(
    viewport_origin: (f32, f32),
    drag_start: (f32, f32),
    drag_end: (f32, f32),
) -> capture::Region {
    let (x0, x1) = (drag_start.0.min(drag_end.0), drag_start.0.max(drag_end.0));
    let (y0, y1) = (drag_start.1.min(drag_end.1), drag_start.1.max(drag_end.1));
    capture::Region {
        x: (viewport_origin.0 + x0).round() as i32,
        y: (viewport_origin.1 + y0).round() as i32,
        width: (x1 - x0).round().max(1.0) as u32,
        height: (y1 - y0).round().max(1.0) as u32,
    }
}
```

- [ ] **Step 8: Run the tests, verify they pass**

Run: `cargo test -p app region_from_drag`
Expected: `test result: ok. 4 passed; 0 failed`

- [ ] **Step 9: Add `selection_after_duplicate` to the existing `editor_screen.rs`**

Add the failing test to its `tests` module:

```rust
#[test]
fn selection_moves_to_the_new_copy_after_duplicate() {
    assert_eq!(selection_after_duplicate(0), 1);
    assert_eq!(selection_after_duplicate(3), 4);
}
```

Add the stub function at module scope, alongside `selection_after_delete`:

```rust
pub fn selection_after_duplicate(duplicated_at: usize) -> usize {
    unimplemented!()
}
```

Run: `cargo test -p app selection_moves_to_the_new_copy_after_duplicate` — expect FAIL, then implement:

```rust
pub fn selection_after_duplicate(duplicated_at: usize) -> usize {
    duplicated_at + 1
}
```

Run: `cargo test -p app selection_moves_to_the_new_copy_after_duplicate` — expect PASS.

- [ ] **Step 10: Wire the two new modules into `main.rs`**

Add near the top of `crates/app/src/main.rs`, alongside the existing `mod` declarations:

```rust
mod selection_overlay;
mod text_tool;
```

- [ ] **Step 11: Run the full test suite**

Run: `cargo test --workspace`
Expected: all tests pass (16 in `editor`, 17 in `app`: the existing 10 plus 3 + 4 new here).

- [ ] **Step 12: Commit**

```bash
git add crates/app/src/text_tool.rs crates/app/src/selection_overlay.rs crates/app/src/editor_screen.rs crates/app/src/main.rs
git commit -m "feat(app): add pure helpers for text placement, area-drag region math, and duplicate selection"
```

---

### Task 5: `capture` crate — monitor geometry helpers

**Files:**
- Modify: `crates/capture/src/lib.rs`

**Interfaces:**
- Produces: `#[derive(Debug, Clone, Copy, PartialEq)]` on `Region` (new derives)
- Produces: `bounding_box(monitors: &[Region]) -> Region` (pure)
- Produces: `monitor_bounds_at(x: i32, y: i32) -> Result<Region, CaptureError>` (needs a live X11 display — manual verify only)
- Produces: `virtual_screen_bounds() -> Result<Region, CaptureError>` (needs a live X11 display — manual verify only)

- [ ] **Step 1: Add the derives and the failing tests for `bounding_box`**

Change the `Region` struct definition:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
```

Add a `tests` module at the end of `crates/capture/src/lib.rs` (this crate has none yet):

```rust
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
}
```

- [ ] **Step 2: Add the stub function**

Add at module scope:

```rust
pub fn bounding_box(monitors: &[Region]) -> Region {
    unimplemented!()
}
```

- [ ] **Step 3: Run the tests, verify they fail**

Run: `cargo test -p capture`
Expected: 4 tests FAIL (panic: `not implemented`).

- [ ] **Step 4: Implement `bounding_box`**

```rust
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
```

- [ ] **Step 5: Run the tests, verify they pass**

Run: `cargo test -p capture`
Expected: `test result: ok. 4 passed; 0 failed`

- [ ] **Step 6: Add `monitor_bounds_at` and `virtual_screen_bounds` (manual-verify only, same rationale as `start_capture` — needs a live X11 display)**

```rust
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
```

- [ ] **Step 7: Run the full workspace build and test suite**

Run: `cargo build --workspace && cargo test --workspace`
Expected: builds clean, all tests pass (20 in `capture`+`editor`+`app` combined at this point: 16 `editor` + 4 `capture` + the `app` count from Task 4).

- [ ] **Step 8: Commit**

```bash
git add crates/capture/src/lib.rs
git commit -m "feat(capture): add monitor bounding-box and lookup helpers"
```

---

### Task 6: `app` — Project screen + Tela Inteira capture (replaces Selecting)

**Files:**
- Create: `crates/app/src/project_screen.rs`
- Delete: `crates/app/src/selection.rs` (its only function, `region_from_window_rect`, is superseded — the app window is no longer the capture region)
- Modify: `crates/app/src/main.rs` (full rewrite of the state machine)

**Interfaces:**
- Consumes: `capture::monitor_bounds_at`, `capture::Region` (Task 5)
- Produces: `project_screen::{ProjectAction, ProjectScreen}` — `ProjectScreen::show(&mut self, ui: &mut egui::Ui) -> Option<ProjectAction>`, `ProjectAction::StartFullScreen`

This task is GUI-only (no automated tests) — same rationale as the original MVP's `app` scaffold tasks. Verify manually per Step 4.

- [ ] **Step 1: Delete the superseded selection module**

```bash
git rm crates/app/src/selection.rs
```

- [ ] **Step 2: Create `project_screen.rs`**

```rust
use eframe::egui;

pub enum ProjectAction {
    StartFullScreen,
}

pub struct ProjectScreen {
    pub fps: u32,
}

impl Default for ProjectScreen {
    fn default() -> Self {
        Self { fps: 20 }
    }
}

impl ProjectScreen {
    const FPS_PRESETS: [u32; 4] = [8, 12, 15, 20];

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<ProjectAction> {
        let mut action = None;
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.heading("Gravar tela");
            ui.label("Escolha a taxa de quadros e grave sua tela ou uma área selecionada.");
            ui.add_space(16.0);

            let (rect, _) = ui.allocate_exact_size(egui::vec2(400.0, 220.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 8.0, egui::Color32::from_gray(40));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Pronto para gravar",
                egui::FontId::proportional(16.0),
                egui::Color32::GRAY,
            );

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.label("FPS");
                for preset in Self::FPS_PRESETS {
                    if ui.selectable_label(self.fps == preset, preset.to_string()).clicked() {
                        self.fps = preset;
                    }
                }
            });

            ui.add_space(16.0);
            if ui.button("Tela Inteira").clicked() {
                action = Some(ProjectAction::StartFullScreen);
            }
        });
        action
    }
}
```

- [ ] **Step 3: Rewrite `main.rs`**

Replace the entire contents of `crates/app/src/main.rs`:

```rust
mod crop_tool;
mod editor_screen;
mod export_screen;
mod project_screen;
mod selection_overlay;
mod text_tool;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use eframe::egui;
use editor::{Frame, FrameList};
use editor_screen::{EditorAction, EditorScreen};
use export_screen::{start_export, ExportJob};
use global_hotkey::{
    hotkey::{Code, HotKey},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use project_screen::{ProjectAction, ProjectScreen};

enum AppState {
    Project(ProjectScreen),
    Recording {
        stop_flag: Arc<AtomicBool>,
        handle: JoinHandle<Result<(), capture::CaptureError>>,
        rx: Receiver<Frame>,
        frames: Vec<Frame>,
        started_at: Instant,
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
        Self {
            _hotkey_manager: manager,
            toggle_hotkey,
            state: AppState::Project(ProjectScreen::default()),
            last_error: None,
        }
    }
}

impl App {
    fn start_recording(&mut self, region: capture::Region, fps: u32) {
        self.last_error = None;
        let (tx, rx) = channel();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let handle = capture::start_capture(region, fps, tx, stop_flag.clone());
        self.state = AppState::Recording { stop_flag, handle, rx, frames: Vec::new(), started_at: Instant::now() };
    }

    /// Stops the in-progress recording (if any) and transitions to
    /// `Editing`, then raises and focuses the main window — the app window
    /// is hidden during `Recording`, so without this the user has no way
    /// to tell where the recording went.
    fn stop_recording(&mut self, ctx: &egui::Context) {
        self.state = match std::mem::replace(&mut self.state, AppState::Project(ProjectScreen::default())) {
            AppState::Recording { stop_flag, handle, mut frames, rx, .. } => {
                stop_flag.store(true, Ordering::Relaxed);
                // A capture error only stops future frames — everything already
                // sent through `rx` is still collected below, so a mid-recording
                // failure never discards frames the user already captured.
                if let Err(e) = handle.join().expect("capture thread panicked") {
                    self.last_error = Some(format!("A gravação parou antes do esperado: {e}"));
                }
                frames.extend(rx.try_iter());
                let frames = FrameList::new(frames);
                let screen = EditorScreen::new(ctx, &frames);
                AppState::Editing { frames, screen }
            }
            other => other,
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.id == self.toggle_hotkey.id() && event.state == global_hotkey::HotKeyState::Pressed {
                if matches!(self.state, AppState::Recording { .. }) {
                    self.stop_recording(&ctx);
                }
            }
        }
        ctx.request_repaint();

        let hide_main_window = matches!(self.state, AppState::Recording { .. });
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(!hide_main_window));

        if let AppState::Recording { rx, frames, .. } = &mut self.state {
            frames.extend(rx.try_iter());
        }

        let mut should_start_full_screen = false;
        let mut should_start_export: Option<PathBuf> = None;

        egui::CentralPanel::default().show(ui, |ui| match &mut self.state {
            AppState::Project(screen) => {
                if let Some(ProjectAction::StartFullScreen) = screen.show(ui) {
                    should_start_full_screen = true;
                }
            }
            AppState::Recording { frames, .. } => {
                ui.centered_and_justified(|ui| {
                    ui.label(format!("Gravando... F9 para parar. ({} frames)", frames.len()));
                });
            }
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
            AppState::Exporting { job, .. } => {
                let (current, total) = *job.progress.lock().unwrap();
                ui.label("Exportando...");
                ui.add(egui::ProgressBar::new(if total == 0 { 0.0 } else { current as f32 / total as f32 }));
            }
            AppState::Done { output_path } => {
                ui.label(format!("Salvo em: {}", output_path.display()));
            }
        });

        if should_start_full_screen {
            if let AppState::Project(screen) = &self.state {
                let fps = screen.fps;
                let window_center = ctx
                    .input(|i| i.viewport().inner_rect)
                    .expect("window position is unavailable on this platform")
                    .center();
                let region = capture::monitor_bounds_at(window_center.x as i32, window_center.y as i32)
                    .expect("could not determine the monitor under the app window");
                self.start_recording(region, fps);
            }
        }

        if let Some(path) = should_start_export {
            self.state = match std::mem::replace(&mut self.state, AppState::Project(ProjectScreen::default())) {
                AppState::Editing { frames, screen } => {
                    let job = start_export(&frames, path.clone());
                    AppState::Exporting { frames, screen, job, output_path: path }
                }
                other => other,
            };
        }

        if let AppState::Exporting { job, .. } = &self.state {
            if job.handle.is_finished() {
                self.state = match std::mem::replace(&mut self.state, AppState::Project(ProjectScreen::default())) {
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
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(true)
            .with_resizable(true)
            .with_transparent(false)
            .with_inner_size([480.0, 420.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native("ScreenToGif", options, Box::new(|_cc| Ok(Box::new(App::default()))))
}
```

(Window grew from `320×240` to `480×420` — the Project screen's 400×220 placeholder plus padding no longer fits the old size.)

- [ ] **Step 4: Build and manually verify**

Run: `cargo build --workspace` — expect a clean build (no warnings about unused `selection_overlay`/`text_tool` items — both modules only contain `pub fn`s used by nothing yet in `main.rs` besides the `mod` declaration, which is enough to avoid a whole-module dead-code warning since their own `#[cfg(test)]` blocks exercise them).

Run: `cargo run -p app`.
Expected: the "Gravar tela" screen appears (FPS buttons, "Pronto para gravar" placeholder, "Tela Inteira" button). Click an FPS preset — it highlights. Click "Tela Inteira" — the window should hide (confirm nothing of the app is visible — if it doesn't, `ViewportCommand::Visible(false)` may not be honored by this window manager; if so, note it here and substitute `ViewportCommand::Minimized(true)` in Step 3 and re-verify). Change something on screen, press F9 — the window reappears, focused, showing "Gravação concluída: N frames capturados." with the *old* thumbnail-row editor (unchanged from the current MVP — the redesigned editor lands in Task 9). Export still works end to end.

- [ ] **Step 5: Commit**

```bash
git add -A crates/app README.md
git commit -m "feat(app): add Project screen and Tela Inteira capture, remove window-as-region selection"
```

---

### Task 7: `app` — recording indicator viewport

**Files:**
- Create: `crates/app/src/recording_indicator.rs`
- Modify: `crates/app/src/main.rs`

**Interfaces:**
- Produces: `recording_indicator::show(ctx: &egui::Context, elapsed_secs: u64, frame_count: usize) -> bool` (returns `true` if the indicator's stop button was clicked)

GUI-only, manual verify.

- [ ] **Step 1: Create `recording_indicator.rs`**

```rust
use eframe::egui;

/// Renders the always-on-top "recording in progress" indicator as its own
/// viewport. Must be called every frame the indicator should stay visible
/// — egui viewports only persist while shown every pass. Returns `true`
/// if the user clicked the indicator's own stop button.
pub fn show(ctx: &egui::Context, elapsed_secs: u64, frame_count: usize) -> bool {
    let mut stop_clicked = false;
    let viewport_id = egui::ViewportId::from_hash_of("recording_indicator");
    ctx.show_viewport_immediate(
        viewport_id,
        egui::ViewportBuilder::default()
            .with_always_on_top()
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_inner_size([240.0, 48.0])
            .with_position([40.0, 40.0]),
        |ui, _class| {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(egui::Color32::from_black_alpha(220)).inner_margin(8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::RED, "●");
                        let mins = elapsed_secs / 60;
                        let secs = elapsed_secs % 60;
                        ui.label(format!("REC · {mins:02}:{secs:02} · {frame_count} frames"));
                        if ui.button("⏹").clicked() {
                            stop_clicked = true;
                        }
                    });
                });
        },
    );
    stop_clicked
}
```

- [ ] **Step 2: Wire it into `main.rs`**

Add `mod recording_indicator;` alongside the other `mod` declarations.

Replace the `AppState::Recording { .. }` render arm:

```rust
AppState::Recording { frames, .. } => {
    ui.centered_and_justified(|ui| {
        ui.label(format!("Gravando... F9 para parar. ({} frames)", frames.len()));
    });
}
```

with:

```rust
AppState::Recording { frames, started_at, .. } => {
    ui.centered_and_justified(|ui| {
        ui.label("Gravando...");
    });
    if recording_indicator::show(&ctx, started_at.elapsed().as_secs(), frames.len()) {
        should_stop_recording = true;
    }
}
```

Declare the new flag alongside the existing ones:

```rust
let mut should_start_full_screen = false;
let mut should_start_export: Option<PathBuf> = None;
let mut should_stop_recording = false;
```

And after the `egui::CentralPanel::default().show(...)` block (alongside the `should_start_full_screen` handling), add:

```rust
if should_stop_recording {
    self.stop_recording(&ctx);
}
```

- [ ] **Step 3: Build and manually verify**

Run: `cargo build --workspace`.

Run: `cargo run -p app`, click "Tela Inteira".
Expected: the main window hides, a small always-on-top box appears in a corner showing `● REC · 00:0X · N frames`, updating every second. Click its `⏹` button — recording stops, main window reappears focused on the editor. Repeat, this time pressing F9 instead of the indicator's button — same result.

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/recording_indicator.rs crates/app/src/main.rs
git commit -m "feat(app): add always-on-top recording indicator viewport"
```

---

### Task 8: `app` — selection overlay + Selecionar Área

**Files:**
- Modify: `crates/app/src/selection_overlay.rs` (add the viewport-rendering function; `region_from_drag` already exists from Task 4)
- Modify: `crates/app/src/project_screen.rs` (add the second button)
- Modify: `crates/app/src/main.rs`

**Interfaces:**
- Consumes: `capture::virtual_screen_bounds` (Task 5), `selection_overlay::region_from_drag` (Task 4)
- Produces: `selection_overlay::show(ctx: &egui::Context, bounds: capture::Region, drag_start: &mut Option<(f32, f32)>) -> Option<capture::Region>`
- Produces: `ProjectAction::StartAreaSelection` (new variant)

GUI-only, manual verify.

- [ ] **Step 1: Add the overlay viewport function to `selection_overlay.rs`**

Append to `crates/app/src/selection_overlay.rs` (below the existing `region_from_drag` and its tests):

```rust
use eframe::egui;

/// Renders the full-desktop selection overlay as its own viewport,
/// spanning `bounds` (the union of every monitor, in desktop coordinates —
/// see `capture::virtual_screen_bounds`). `drag_start` is owned by the
/// caller (`AppState::SelectingArea`) so it survives across frames, the
/// same pattern `crop_tool`'s callers use. Returns the selected region
/// once the user releases a non-degenerate drag; the caller stops calling
/// this function once that happens, which closes the overlay.
pub fn show(
    ctx: &egui::Context,
    bounds: capture::Region,
    drag_start: &mut Option<(f32, f32)>,
) -> Option<capture::Region> {
    let mut result = None;
    let viewport_id = egui::ViewportId::from_hash_of("selection_overlay");
    ctx.show_viewport_immediate(
        viewport_id,
        egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_inner_size([bounds.width as f32, bounds.height as f32])
            .with_position([bounds.x as f32, bounds.y as f32]),
        |ui, _class| {
            let response = ui.allocate_response(ui.available_size(), egui::Sense::drag());
            ui.painter().rect_filled(response.rect, 0.0, egui::Color32::from_black_alpha(120));

            if response.drag_started() {
                *drag_start = ui.input(|i| i.pointer.interact_pos()).map(|p| (p.x, p.y));
            }
            if let (Some(start), Some(current)) = (*drag_start, ui.input(|i| i.pointer.interact_pos())) {
                let rect = egui::Rect::from_two_pos(egui::pos2(start.0, start.1), current);
                ui.painter().rect_filled(rect, 0.0, egui::Color32::from_black_alpha(60));
                ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(2.0, egui::Color32::YELLOW), egui::StrokeKind::Outside);
            }

            if response.drag_stopped() {
                if let (Some(start), Some(end)) = (*drag_start, ui.input(|i| i.pointer.interact_pos())) {
                    let region = region_from_drag((bounds.x as f32, bounds.y as f32), start, (end.x, end.y));
                    if region.width > 1 && region.height > 1 {
                        result = Some(region);
                    }
                }
                *drag_start = None;
            }
        },
    );
    result
}
```

- [ ] **Step 2: Add the second button to `project_screen.rs`**

```rust
pub enum ProjectAction {
    StartFullScreen,
    StartAreaSelection,
}
```

Replace:

```rust
ui.add_space(16.0);
if ui.button("Tela Inteira").clicked() {
    action = Some(ProjectAction::StartFullScreen);
}
```

with:

```rust
ui.add_space(16.0);
ui.horizontal(|ui| {
    if ui.button("Tela Inteira").clicked() {
        action = Some(ProjectAction::StartFullScreen);
    }
    if ui.button("Selecionar Área").clicked() {
        action = Some(ProjectAction::StartAreaSelection);
    }
});
```

- [ ] **Step 3: Wire `SelectingArea` into `main.rs`**

Insert a new variant into `AppState`, right after `Project(ProjectScreen)` and before `Recording { ... }`:

```rust
SelectingArea {
    fps: u32,
    drag_start: Option<(f32, f32)>,
},
```

Extend the hide-main-window check:

```rust
let hide_main_window = matches!(self.state, AppState::Recording { .. } | AppState::SelectingArea { .. });
```

Replace the `AppState::Project(screen)` match arm:

```rust
AppState::Project(screen) => {
    if let Some(ProjectAction::StartFullScreen) = screen.show(ui) {
        should_start_full_screen = true;
    }
}
```

with:

```rust
AppState::Project(screen) => match screen.show(ui) {
    Some(ProjectAction::StartFullScreen) => should_start_full_screen = true,
    Some(ProjectAction::StartAreaSelection) => {
        let fps = screen.fps;
        self.state = AppState::SelectingArea { fps, drag_start: None };
    }
    None => {}
},
```

Note this last arm mutates `self.state` from *inside* the `match &mut self.state { ... }` — that compiles because Rust's disjoint closure captures (stable since the 2021 edition, which this crate uses) let `self.state` and the other `self` fields read elsewhere in the same closure be borrowed independently, and the borrow behind `screen` ends at its last use (`screen.fps`), before the reassignment.

Add the new render arm (placed after the `AppState::Project` arm):

```rust
AppState::SelectingArea { fps, drag_start } => {
    ui.centered_and_justified(|ui| {
        ui.label("Arraste sobre a tela para escolher a área. Esc para cancelar.");
    });
    let bounds = capture::virtual_screen_bounds().expect("could not enumerate monitors");
    if let Some(region) = selection_overlay::show(&ctx, bounds, drag_start) {
        should_start_region_recording = Some((region, *fps));
    }
}
```

Declare the new flag alongside the others:

```rust
let mut should_start_region_recording: Option<(capture::Region, u32)> = None;
```

And after the `should_start_full_screen` handling block, add:

```rust
if let Some((region, fps)) = should_start_region_recording {
    self.start_recording(region, fps);
}
```

- [ ] **Step 4: Build and manually verify**

Run: `cargo build --workspace`.

Run: `cargo run -p app`, click "Selecionar Área".
Expected: the app window hides, the whole screen darkens. Drag a rectangle over any region — it highlights live with a yellow border. Release — the overlay closes and the REC indicator (Task 7) appears; the captured region should match what was dragged (verify later, once Editing shows real thumbnails, that the captured content matches the dragged area — or check now by exporting and opening the GIF). Stop via F9 or the indicator; lands on the editor as before.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/selection_overlay.rs crates/app/src/project_screen.rs crates/app/src/main.rs
git commit -m "feat(app): add full-desktop selection overlay for Selecionar Área"
```

---

### Task 9: `app` — editor screen relayout (Selecionar tool: Duplicar, reorder, delete) + Nova gravação

**Files:**
- Modify: `crates/app/src/editor_screen.rs` (full rewrite)
- Modify: `crates/app/src/main.rs`

**Interfaces:**
- Consumes: `editor::FrameList::duplicate` (Task 1), `editor_screen::selection_after_duplicate` (Task 4, carried over into this rewrite)
- Produces: `EditorAction::Duplicate(usize)` (new variant, alongside the existing `Delete`/`Reorder`)

GUI-only, manual verify.

- [ ] **Step 1: Rewrite `editor_screen.rs`**

Replace the entire contents of `crates/app/src/editor_screen.rs`:

```rust
use eframe::egui;
use editor::FrameList;

pub enum EditorAction {
    Delete(usize),
    Reorder(usize, usize),
    Duplicate(usize),
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

    /// Renders the two-column editor body (large preview + tool sidebar)
    /// and the bottom filmstrip. Returns an action once the user triggers
    /// one via the sidebar.
    pub fn show(&mut self, ui: &mut egui::Ui, frames: &FrameList) -> Option<EditorAction> {
        let mut action = None;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_min_width((ui.available_width() - 220.0).max(0.0));
                if let Some(texture) = self.textures.get(self.selected) {
                    ui.add(egui::Image::new(texture).max_size(ui.available_size()));
                }
            });

            ui.separator();

            ui.vertical(|ui| {
                ui.set_width(200.0);
                ui.label(format!("FRAME {} DE {}", self.selected + 1, frames.len()));
                ui.add_space(8.0);
                if ui.button("Duplicar").clicked() {
                    action = Some(EditorAction::Duplicate(self.selected));
                }
                if ui.add_enabled(self.selected > 0, egui::Button::new("◀ Mover")).clicked() {
                    action = Some(EditorAction::Reorder(self.selected, self.selected - 1));
                }
                if ui.add_enabled(self.selected + 1 < frames.len(), egui::Button::new("Mover ▶")).clicked() {
                    action = Some(EditorAction::Reorder(self.selected, self.selected + 1));
                }
                if ui.add_enabled(!frames.is_empty(), egui::Button::new("Excluir frame")).clicked() {
                    action = Some(EditorAction::Delete(self.selected));
                }
            });
        });

        ui.separator();

        egui::ScrollArea::horizontal().max_height(90.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, (texture, frame)) in self.textures.iter().zip(frames.frames()).enumerate() {
                    ui.vertical(|ui| {
                        let response = ui.add(
                            egui::Button::image(egui::Image::new(texture).fit_to_exact_size(egui::vec2(80.0, 60.0)))
                                .selected(i == self.selected),
                        );
                        if response.clicked() {
                            self.selected = i;
                        }
                        ui.label(format!("{} · {}ms", i + 1, frame.timestamp_ms));
                    });
                }
            });
        });

        action
    }

    pub fn apply_delete(&mut self, index: usize) {
        let _ = self.textures.remove(index); // dropping frees the GPU texture
        self.selected = selection_after_delete(self.selected, index, self.textures.len());
    }

    pub fn apply_reorder(&mut self, from: usize, to: usize) {
        let texture = self.textures.remove(from);
        self.textures.insert(to, texture);
        self.selected = to;
    }
}

/// Computes which thumbnail should stay selected after deleting the frame
/// at `deleted`, given the previously `selected` index and the list's
/// `remaining_len` after the deletion.
pub fn selection_after_delete(selected: usize, deleted: usize, remaining_len: usize) -> usize {
    if remaining_len == 0 {
        0
    } else if deleted < selected {
        selected - 1
    } else {
        selected.min(remaining_len - 1)
    }
}

/// Computes which thumbnail should be selected right after duplicating the
/// frame at `duplicated_at` — the new copy, inserted immediately after it.
pub fn selection_after_duplicate(duplicated_at: usize) -> usize {
    duplicated_at + 1
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

    #[test]
    fn selection_moves_to_the_new_copy_after_duplicate() {
        assert_eq!(selection_after_duplicate(0), 1);
        assert_eq!(selection_after_duplicate(3), 4);
    }
}
```

- [ ] **Step 2: Rewrite the `Editing` render arm in `main.rs`**

Replace:

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

with:

```rust
AppState::Editing { frames, screen } => {
    ui.horizontal(|ui| {
        ui.heading("ScreenToGif");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
        });
    });
    ui.horizontal(|ui| {
        if ui.link("← Nova gravação").clicked() {
            should_return_to_project = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(format!("{} frames", frames.len()));
        });
    });
    if let Some(msg) = &self.last_error {
        ui.colored_label(egui::Color32::RED, msg);
    }
    ui.separator();
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
            EditorAction::Duplicate(i) => {
                frames.duplicate(i).expect("index came from the UI, must be valid");
                *screen = EditorScreen::new(&ctx, frames);
                screen.selected = editor_screen::selection_after_duplicate(i);
            }
        }
    }
}
```

Declare the new flag alongside the others:

```rust
let mut should_return_to_project = false;
```

And after the `should_start_export` handling block, add:

```rust
if should_return_to_project {
    self.state = AppState::Project(ProjectScreen::default());
}
```

- [ ] **Step 3: Build and manually verify**

Run: `cargo build --workspace && cargo test --workspace` — expect a clean build and all tests passing.

Run: `cargo run -p app`, record a short clip via "Tela Inteira".
Expected: the editor now shows a top bar ("ScreenToGif" / "Exportar"), a second row ("← Nova gravação" / "N frames"), a large preview on the left, a sidebar on the right with "FRAME X DE N", Duplicar/◀ Mover/Mover ▶/Excluir frame, and a filmstrip with time labels at the bottom. Click a thumbnail — preview and frame count update. Click "Duplicar" — frame count increases by one, the new copy is selected. Click "← Nova gravação" — returns to the Project screen, discarding the session. Export still works.

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/editor_screen.rs crates/app/src/main.rs
git commit -m "feat(app): relayout editor screen (preview + sidebar + filmstrip), add Duplicar and Nova gravação"
```

---

### Task 10: `app` — tool selector + Recortar relocated

**Files:**
- Modify: `crates/app/src/editor_screen.rs`
- Modify: `crates/app/src/main.rs`

**Interfaces:**
- Consumes: `crop_tool::crop_rect_from_drag` (existing, unchanged)
- Produces: `editor_screen::Tool` (new enum: `Selecionar | Recortar`), `EditorAction::Crop(editor::CropRect)` (new variant)

GUI-only, manual verify.

- [ ] **Step 1: Add the `Tool` enum and sidebar tool state**

Add above `EditorAction`:

```rust
#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Selecionar,
    Recortar,
}
```

Add `Crop(editor::CropRect)` to `EditorAction`:

```rust
pub enum EditorAction {
    Delete(usize),
    Reorder(usize, usize),
    Duplicate(usize),
    Crop(editor::CropRect),
}
```

Add fields to `EditorScreen` and initialize them in `new`:

```rust
pub struct EditorScreen {
    textures: Vec<egui::TextureHandle>,
    pub selected: usize,
    tool: Tool,
    crop_drag_start: Option<egui::Pos2>,
}
```

```rust
Self { textures, selected: 0, tool: Tool::Selecionar, crop_drag_start: None }
```

- [ ] **Step 2: Add the tool-selector row and per-tool sidebar panel**

Replace the sidebar's `ui.vertical(|ui| { ui.set_width(200.0); ... })` block body:

```rust
ui.vertical(|ui| {
    ui.set_width(200.0);

    ui.horizontal(|ui| {
        if ui.selectable_label(self.tool == Tool::Selecionar, "Selecionar").clicked() {
            self.tool = Tool::Selecionar;
        }
        if ui.selectable_label(self.tool == Tool::Recortar, "Recortar").clicked() {
            self.tool = Tool::Recortar;
            self.crop_drag_start = None;
        }
    });
    ui.add_space(8.0);

    match self.tool {
        Tool::Selecionar => {
            ui.label(format!("FRAME {} DE {}", self.selected + 1, frames.len()));
            ui.add_space(8.0);
            if ui.button("Duplicar").clicked() {
                action = Some(EditorAction::Duplicate(self.selected));
            }
            if ui.add_enabled(self.selected > 0, egui::Button::new("◀ Mover")).clicked() {
                action = Some(EditorAction::Reorder(self.selected, self.selected - 1));
            }
            if ui.add_enabled(self.selected + 1 < frames.len(), egui::Button::new("Mover ▶")).clicked() {
                action = Some(EditorAction::Reorder(self.selected, self.selected + 1));
            }
            if ui.add_enabled(!frames.is_empty(), egui::Button::new("Excluir frame")).clicked() {
                action = Some(EditorAction::Delete(self.selected));
            }
        }
        Tool::Recortar => {
            ui.label("Arraste sobre o preview para recortar.");
        }
    }
});
```

- [ ] **Step 3: Add crop-drag handling to the preview column**

Replace the preview column's body:

```rust
ui.vertical(|ui| {
    ui.set_min_width((ui.available_width() - 220.0).max(0.0));
    if let Some(texture) = self.textures.get(self.selected) {
        ui.add(egui::Image::new(texture).max_size(ui.available_size()));
    }
});
```

with:

```rust
ui.vertical(|ui| {
    ui.set_min_width((ui.available_width() - 220.0).max(0.0));
    if let Some(texture) = self.textures.get(self.selected) {
        let sense = if self.tool == Tool::Recortar { egui::Sense::drag() } else { egui::Sense::hover() };
        let image_response = ui.add(egui::Image::new(texture).max_size(ui.available_size()).sense(sense));

        if self.tool == Tool::Recortar {
            if image_response.drag_started() {
                self.crop_drag_start = ui.input(|i| i.pointer.interact_pos());
            }
            if let (Some(start), Some(current)) = (self.crop_drag_start, ui.input(|i| i.pointer.interact_pos())) {
                let rect_on_screen = egui::Rect::from_two_pos(start, current);
                ui.painter().rect_stroke(
                    rect_on_screen,
                    0.0,
                    egui::Stroke::new(2.0, egui::Color32::YELLOW),
                    egui::StrokeKind::Outside,
                );
            }
            if image_response.drag_stopped() {
                if let (Some(start), Some(end)) = (self.crop_drag_start, ui.input(|i| i.pointer.interact_pos())) {
                    let rect = image_response.rect;
                    let to_local = |p: egui::Pos2| (p.x - rect.min.x, p.y - rect.min.y);
                    let frame = &frames.frames()[self.selected];
                    action = Some(EditorAction::Crop(crate::crop_tool::crop_rect_from_drag(
                        to_local(start),
                        to_local(end),
                        (rect.width(), rect.height()),
                        (frame.image.width(), frame.image.height()),
                    )));
                    self.crop_drag_start = None;
                }
            }
        }
    }
});
```

- [ ] **Step 4: Handle `EditorAction::Crop` in `main.rs`**

Add to the `match action` block in the `Editing` arm:

```rust
EditorAction::Crop(rect) => {
    frames.crop(rect).expect("crop rect came from the UI, must be valid");
    *screen = EditorScreen::new(&ctx, frames);
}
```

- [ ] **Step 5: Build and manually verify**

Run: `cargo build --workspace && cargo test --workspace`.

Run: `cargo run -p app`, record, in the editor click "Recortar", drag over the preview.
Expected: a yellow rectangle follows the drag; on release, the preview and every thumbnail shrink to the cropped region; switching back to "Selecionar" shows the usual panel again.

- [ ] **Step 6: Commit**

```bash
git add crates/app/src/editor_screen.rs crates/app/src/main.rs
git commit -m "feat(app): add tool selector and relocate Recortar into the sidebar"
```

---

### Task 11: `app` — Blur tool

**Files:**
- Modify: `crates/app/src/editor_screen.rs`
- Modify: `crates/app/src/main.rs`

**Interfaces:**
- Consumes: `editor::FrameList::blur` (Task 2), `crop_tool::crop_rect_from_drag` (reused for the drag math — identical shape to Recortar's)
- Produces: `Tool::Blur` (new variant), `EditorAction::Blur(editor::CropRect, f32)` (new variant)

GUI-only, manual verify.

- [ ] **Step 1: Extend `Tool` and `EditorAction`, add sidebar state**

```rust
#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Selecionar,
    Recortar,
    Blur,
}
```

```rust
pub enum EditorAction {
    Delete(usize),
    Reorder(usize, usize),
    Duplicate(usize),
    Crop(editor::CropRect),
    Blur(editor::CropRect, f32),
}
```

Add fields to `EditorScreen`:

```rust
pub struct EditorScreen {
    textures: Vec<egui::TextureHandle>,
    pub selected: usize,
    tool: Tool,
    crop_drag_start: Option<egui::Pos2>,
    blur_drag_start: Option<egui::Pos2>,
    blur_sigma: f32,
}
```

Initialize in `new`:

```rust
Self {
    textures,
    selected: 0,
    tool: Tool::Selecionar,
    crop_drag_start: None,
    blur_drag_start: None,
    blur_sigma: 4.0,
}
```

- [ ] **Step 2: Add the "Blur" selector button and its panel**

In the tool-selector row, add a third button:

```rust
if ui.selectable_label(self.tool == Tool::Blur, "Blur").clicked() {
    self.tool = Tool::Blur;
    self.blur_drag_start = None;
}
```

Add a `Tool::Blur` arm to the `match self.tool { ... }` block:

```rust
Tool::Blur => {
    ui.label("Arraste sobre o preview para borrar.");
    ui.add(egui::Slider::new(&mut self.blur_sigma, 1.0..=20.0).text("Intensidade"));
}
```

- [ ] **Step 3: Add blur-drag handling to the preview column**

Extend the `sense` computation and drag branch added in Task 10:

```rust
let sense = match self.tool {
    Tool::Recortar | Tool::Blur => egui::Sense::drag(),
    Tool::Selecionar => egui::Sense::hover(),
};
let image_response = ui.add(egui::Image::new(texture).max_size(ui.available_size()).sense(sense));

if self.tool == Tool::Recortar {
    if image_response.drag_started() {
        self.crop_drag_start = ui.input(|i| i.pointer.interact_pos());
    }
    if let (Some(start), Some(current)) = (self.crop_drag_start, ui.input(|i| i.pointer.interact_pos())) {
        let rect_on_screen = egui::Rect::from_two_pos(start, current);
        ui.painter().rect_stroke(
            rect_on_screen,
            0.0,
            egui::Stroke::new(2.0, egui::Color32::YELLOW),
            egui::StrokeKind::Outside,
        );
    }
    if image_response.drag_stopped() {
        if let (Some(start), Some(end)) = (self.crop_drag_start, ui.input(|i| i.pointer.interact_pos())) {
            let rect = image_response.rect;
            let to_local = |p: egui::Pos2| (p.x - rect.min.x, p.y - rect.min.y);
            let frame = &frames.frames()[self.selected];
            action = Some(EditorAction::Crop(crate::crop_tool::crop_rect_from_drag(
                to_local(start),
                to_local(end),
                (rect.width(), rect.height()),
                (frame.image.width(), frame.image.height()),
            )));
            self.crop_drag_start = None;
        }
    }
}

if self.tool == Tool::Blur {
    if image_response.drag_started() {
        self.blur_drag_start = ui.input(|i| i.pointer.interact_pos());
    }
    if let (Some(start), Some(current)) = (self.blur_drag_start, ui.input(|i| i.pointer.interact_pos())) {
        let rect_on_screen = egui::Rect::from_two_pos(start, current);
        ui.painter().rect_stroke(
            rect_on_screen,
            0.0,
            egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE),
            egui::StrokeKind::Outside,
        );
    }
    if image_response.drag_stopped() {
        if let (Some(start), Some(end)) = (self.blur_drag_start, ui.input(|i| i.pointer.interact_pos())) {
            let rect = image_response.rect;
            let to_local = |p: egui::Pos2| (p.x - rect.min.x, p.y - rect.min.y);
            let frame = &frames.frames()[self.selected];
            action = Some(EditorAction::Blur(
                crate::crop_tool::crop_rect_from_drag(
                    to_local(start),
                    to_local(end),
                    (rect.width(), rect.height()),
                    (frame.image.width(), frame.image.height()),
                ),
                self.blur_sigma,
            ));
            self.blur_drag_start = None;
        }
    }
}
```

- [ ] **Step 4: Handle `EditorAction::Blur` in `main.rs`**

Add to the `match action` block:

```rust
EditorAction::Blur(rect, sigma) => {
    frames.blur(rect, sigma).expect("blur rect came from the UI, must be valid");
    *screen = EditorScreen::new(&ctx, frames);
}
```

- [ ] **Step 5: Build and manually verify**

Run: `cargo build --workspace && cargo test --workspace`.

Run: `cargo run -p app`, record, click "Blur", adjust the intensity slider, drag over part of the preview.
Expected: a light-blue rectangle follows the drag; on release, that region is visibly blurred in the preview and in every thumbnail, the rest of the frame untouched.

- [ ] **Step 6: Commit**

```bash
git add crates/app/src/editor_screen.rs crates/app/src/main.rs
git commit -m "feat(app): add Blur tool"
```

---

### Task 12: `app` — Texto tool

**Files:**
- Modify: `crates/app/src/editor_screen.rs`
- Modify: `crates/app/src/main.rs`

**Interfaces:**
- Consumes: `editor::FrameList::add_text` (Task 3), `text_tool::text_position_from_click` (Task 4)
- Produces: `Tool::Texto` (new variant), `EditorAction::AddText { position: (u32, u32), text: String, font_size: f32 }` (new variant)

GUI-only, manual verify.

- [ ] **Step 1: Extend `Tool` and `EditorAction`, add sidebar state**

```rust
#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Selecionar,
    Recortar,
    Blur,
    Texto,
}
```

```rust
pub enum EditorAction {
    Delete(usize),
    Reorder(usize, usize),
    Duplicate(usize),
    Crop(editor::CropRect),
    Blur(editor::CropRect, f32),
    AddText { position: (u32, u32), text: String, font_size: f32 },
}
```

Add a field to `EditorScreen` and initialize it in `new`. The full struct, with the new `text_input` field added at the end:

```rust
pub struct EditorScreen {
    textures: Vec<egui::TextureHandle>,
    pub selected: usize,
    tool: Tool,
    crop_drag_start: Option<egui::Pos2>,
    blur_drag_start: Option<egui::Pos2>,
    blur_sigma: f32,
    text_input: String,
}
```

And the full initializer in `new`:

```rust
Self {
    textures,
    selected: 0,
    tool: Tool::Selecionar,
    crop_drag_start: None,
    blur_drag_start: None,
    blur_sigma: 4.0,
    text_input: String::new(),
}
```

- [ ] **Step 2: Add the "Texto" selector button and its panel**

```rust
if ui.selectable_label(self.tool == Tool::Texto, "Texto").clicked() {
    self.tool = Tool::Texto;
}
```

```rust
Tool::Texto => {
    ui.label("Clique no preview para posicionar o texto.");
    ui.text_edit_singleline(&mut self.text_input);
}
```

- [ ] **Step 3: Add click-to-place handling to the preview column**

Extend `sense`:

```rust
let sense = match self.tool {
    Tool::Recortar | Tool::Blur => egui::Sense::drag(),
    Tool::Texto => egui::Sense::click(),
    Tool::Selecionar => egui::Sense::hover(),
};
```

Add, after the `Tool::Blur` drag-handling block:

```rust
if self.tool == Tool::Texto && image_response.clicked() && !self.text_input.trim().is_empty() {
    if let Some(click) = ui.input(|i| i.pointer.interact_pos()) {
        let rect = image_response.rect;
        let local = (click.x - rect.min.x, click.y - rect.min.y);
        let frame = &frames.frames()[self.selected];
        let position = crate::text_tool::text_position_from_click(
            local,
            (rect.width(), rect.height()),
            (frame.image.width(), frame.image.height()),
        );
        action = Some(EditorAction::AddText { position, text: self.text_input.clone(), font_size: 24.0 });
        self.text_input.clear();
    }
}
```

- [ ] **Step 4: Handle `EditorAction::AddText` in `main.rs`**

Add to the `match action` block:

```rust
EditorAction::AddText { position, text, font_size } => {
    frames
        .add_text(position, text, font_size, [255, 255, 255, 255])
        .expect("text came from the UI, must be non-empty");
    *screen = EditorScreen::new(&ctx, frames);
}
```

- [ ] **Step 5: Build and manually verify**

Run: `cargo build --workspace && cargo test --workspace`.

Run: `cargo run -p app`, record, click "Texto", type something in the field, click a spot on the preview.
Expected: the typed text is burned into the preview at the clicked position, in every thumbnail; the input field clears after placing.

- [ ] **Step 6: Commit**

```bash
git add crates/app/src/editor_screen.rs crates/app/src/main.rs
git commit -m "feat(app): add Texto tool"
```

---

### Task 13: `app` — Prévia (play) button

**Files:**
- Modify: `crates/app/src/editor_screen.rs`

**Interfaces:**
- Produces: `editor_screen::frame_index_at(timestamps_ms: &[u64], elapsed_ms: u64) -> usize` (pure, tested)

- [ ] **Step 1: Add the failing tests for `frame_index_at`**

Add to the `tests` module:

```rust
#[test]
fn frame_index_at_start_is_zero() {
    assert_eq!(frame_index_at(&[0, 100, 200], 0), 0);
}

#[test]
fn frame_index_at_picks_the_latest_timestamp_not_after_elapsed() {
    assert_eq!(frame_index_at(&[0, 100, 200], 150), 1);
}

#[test]
fn frame_index_at_loops_back_after_the_last_timestamp() {
    assert_eq!(frame_index_at(&[0, 100, 200], 250), 0);
}

#[test]
fn frame_index_at_of_empty_list_is_zero() {
    assert_eq!(frame_index_at(&[], 500), 0);
}
```

- [ ] **Step 2: Add the stub function**

```rust
/// Given ascending frame timestamps (as captured) and elapsed time since
/// playback started, returns which frame index should be showing, looping
/// back to the start once elapsed passes the last timestamp.
pub fn frame_index_at(timestamps_ms: &[u64], elapsed_ms: u64) -> usize {
    unimplemented!()
}
```

- [ ] **Step 3: Run the tests, verify they fail**

Run: `cargo test -p app frame_index_at`
Expected: 4 tests FAIL (panic: `not implemented`).

- [ ] **Step 4: Implement `frame_index_at`**

```rust
pub fn frame_index_at(timestamps_ms: &[u64], elapsed_ms: u64) -> usize {
    if timestamps_ms.is_empty() {
        return 0;
    }
    let total = timestamps_ms.last().copied().unwrap_or(0).max(1);
    let looped = elapsed_ms % total;
    timestamps_ms.iter().rposition(|&t| t <= looped).unwrap_or(0)
}
```

- [ ] **Step 5: Run the tests, verify they pass**

Run: `cargo test -p app frame_index_at`
Expected: `test result: ok. 4 passed; 0 failed`

- [ ] **Step 6: Wire the Prévia button and playback into `show`**

Add fields to `EditorScreen` and initialize in `new`. The full struct, with `playing` and `play_started_at` added at the end:

```rust
pub struct EditorScreen {
    textures: Vec<egui::TextureHandle>,
    pub selected: usize,
    tool: Tool,
    crop_drag_start: Option<egui::Pos2>,
    blur_drag_start: Option<egui::Pos2>,
    blur_sigma: f32,
    text_input: String,
    playing: bool,
    play_started_at: Option<std::time::Instant>,
}
```

And the full initializer in `new`:

```rust
Self {
    textures,
    selected: 0,
    tool: Tool::Selecionar,
    crop_drag_start: None,
    blur_drag_start: None,
    blur_sigma: 4.0,
    text_input: String::new(),
    playing: false,
    play_started_at: None,
}
```

At the top of `show`, above the tool-selector row (inside the sidebar's `ui.vertical` block), add the Prévia toggle:

```rust
if ui.button(if self.playing { "⏸ Pausar" } else { "▷ Prévia" }).clicked() {
    self.playing = !self.playing;
    self.play_started_at = if self.playing { Some(std::time::Instant::now()) } else { None };
}
ui.add_space(8.0);
```

Change the preview column to display the playback frame while playing, falling back to `self.selected` otherwise. Replace:

```rust
if let Some(texture) = self.textures.get(self.selected) {
```

with:

```rust
let display_index = match self.play_started_at {
    Some(start) => {
        let timestamps: Vec<u64> = frames.frames().iter().map(|f| f.timestamp_ms).collect();
        frame_index_at(&timestamps, start.elapsed().as_millis() as u64)
    }
    None => self.selected,
};
if let Some(texture) = self.textures.get(display_index) {
```

(The crop/blur/text drag math further down still reads `frames.frames()[self.selected]` and `self.textures.get(self.selected)` for the filmstrip highlight — those are intentionally unaffected by playback, which only swaps the big preview image.)

- [ ] **Step 7: Build and manually verify**

Run: `cargo build --workspace && cargo test --workspace`.

Run: `cargo run -p app`, record a few seconds of visible motion, click "▷ Prévia".
Expected: the preview cycles through the captured frames in a loop at roughly the original capture pace; "⏸ Pausar" stops it back on the currently-selected frame.

- [ ] **Step 8: Commit**

```bash
git add crates/app/src/editor_screen.rs
git commit -m "feat(app): add Prévia loop-playback button"
```

---

### Task 14: README — rewrite the manual checklist for the new flow

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Rewrite the "Fluxo de uso" and "Testes automatizados" sections**

Replace the `## Fluxo de uso` section:

```markdown
## Fluxo de uso

1. Na tela "Gravar tela", escolha o FPS (8/12/15/20) e clique em **Tela Inteira** ou **Selecionar Área**.
   - **Tela Inteira**: grava o monitor onde a janela do app está.
   - **Selecionar Área**: a tela escurece — arraste um retângulo sobre a região desejada; ao soltar, a gravação começa.
2. Durante a gravação, um indicador flutuante mostra `● REC · MM:SS · N frames`. Clique no botão de parar do indicador, ou pressione **F9**, a qualquer momento.
3. A janela principal volta ao primeiro plano, já na tela de edição, mostrando o resultado.
4. No editor: a miniatura selecionada aparece grande à esquerda; a filmstrip embaixo lista todos os frames (clique para selecionar). No painel à direita, escolha a ferramenta:
   - **Selecionar**: Duplicar, mover (◀/▶), excluir o frame atual.
   - **Recortar**: arraste sobre o preview para cortar todos os frames.
   - **Blur**: ajuste a intensidade, arraste sobre o preview para borrar uma região em todos os frames.
   - **Texto**: digite o texto, clique no preview para posicioná-lo em todos os frames.
   - **▷ Prévia** reproduz os frames em loop no preview.
5. Clique em **Exportar**, escolha onde salvar; acompanhe a barra de progresso até "Salvo em: ...". "← Nova gravação" descarta a sessão atual e volta à tela inicial.
```

Replace the `## Testes automatizados` section's last line (about `capture`) to also cover the new manual-only pieces:

```markdown
## Testes automatizados

    cargo test --workspace

`capture` e as partes de janela/viewport do `app` (overlay de seleção, indicador de gravação, esconder/focar a janela principal) não têm testes automatizados — dependem de um display X11 real. Use o checklist manual abaixo para verificá-las.

## Checklist manual end-to-end

1. Tela Inteira: grava, indicador aparece e conta corretamente, F9 para, editor mostra o resultado com a janela principal em primeiro plano.
2. Selecionar Área: overlay cobre a tela, arrasto mostra o retângulo em tempo real, gravação começa só na área escolhida.
3. No editor: exercite Selecionar (duplicar/mover/excluir), Recortar, Blur, Texto e Prévia, nessa ordem, sobre a mesma gravação.
4. Exportar e abrir o GIF resultante — confirme que ele reflete todas as edições (frame duplicado, corte, blur, texto, ordem).
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: rewrite manual checklist for the redesigned capture flow and editor"
```
