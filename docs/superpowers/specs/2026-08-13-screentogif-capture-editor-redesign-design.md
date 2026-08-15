# ScreenToGif Capture Flow & Editor Redesign — Design

## Context

The MVP built from `docs/superpowers/specs/2026-08-12-screentogif-rust-linux-design.md` is functionally complete (capture, delete/reorder/crop, GIF export) but fails on usability:

1. **Region selection isn't real selection.** Today the app's own window *is* the capture region — the user drags/resizes the whole app window over the desktop, which is confusing and doesn't match how any comparable tool works.
2. **The editor UI isn't intuitive.** A single unstructured column of thumbnail buttons, action buttons, and a preview doesn't read as an editor.
3. **No confirmation of where the recording went.** After stopping, the user can't tell what happened to the capture — there's no strong visual handoff from "recording" to "here's your result."

The user prototyped the intended flow and screens (a web mockup, screens below), which this design adapts to the native Rust/X11/egui app. Prototype reference: "GIF Studio" — a "Gravar tela" screen (FPS presets, big "Iniciar gravação" button), a floating "● REC · 00:08 · 172 frames" indicator during capture, and an editor with a large preview, a right-hand tool panel (Selecionar / Recortar / Texto), and a bottom filmstrip of frame thumbnails.

This spec covers a **restructuring of the app's window architecture and state machine**, plus **new editor operations** (duplicate, text, blur) that the prototype introduced. It supersedes the "app scaffold" and "editor screen" sections of the original spec; the `editor`/`capture`/`encoder` crate boundaries and the GIF-only export scope are unchanged.

## Goals

- Real region selection: a full-screen overlay the user drags a rectangle over, independent of the app's own window.
- A "Tela Inteira" (entire screen) capture mode alongside region selection.
- A visible, always-on-top recording indicator (elapsed time + frame count + stop control) so the user always knows recording is active and how to stop it.
- On stop, the app window returns to the foreground already showing the result — no ambiguity about "where the recording went."
- An editor layout modeled on mainstream video editors: large preview, a tool panel, a frame filmstrip — instead of an unstructured stack of buttons.
- New editor tools: **Duplicar** (duplicate frame), **Texto** (burn in text, all frames), **Blur** (blur a region, all frames), and a **Prévia** (play) button that loops the frames in the preview.

## Non-goals (explicitly out of scope for this pass)

- Per-frame delay/timing editing (frames keep using their captured timestamps).
- Drag-and-drop reordering in the filmstrip — reorder stays as ←/→ buttons; drag-and-drop is a later pass.
- Picking a specific window to capture (the prototype's browser-native "Window" tab is a `getDisplayMedia` artifact, not a deliberate product decision) — only "Tela Inteira" and "Selecionar Área" (freeform rectangle).
- A monitor picker for "Tela Inteira" — it captures the monitor the app window is currently on. (A "Selecionar Área" drag crossing monitor boundaries is not specially handled or prevented — see the known limitation noted under Selection overlay.)
- Project save/load to disk (still out of scope, per the original spec).
- Wayland (still X11-only).

## Architecture: window/viewport model

The current app is a single `eframe` window. This redesign needs **three coordinated viewports**, using `egui`'s multi-viewport support (`ctx.show_viewport_deferred` / immediate viewports — new usage for this codebase):

1. **Main window** — shows the `Project` and `Editing` (and `Exporting`/`Done`) screens. Hidden (not just minimized — actually hidden, so it never appears in a capture) while `SelectingArea` or `Recording` is active.
2. **Selection overlay** (transient, `SelectingArea` state only) — a borderless, transparent, click-through-except-for-drag viewport spanning the union of all monitors' bounds. Darkens the screen, highlights the dragged rectangle live, and closes the instant the drag ends.
3. **Recording indicator** (transient, `Recording` state only) — a small, always-on-top, borderless viewport in a screen corner: `● REC · MM:SS · N frames` plus a stop control. Exists for both "Tela Inteira" and "Selecionar Área" recordings.

When recording stops (via the indicator's stop control or the global F9 hotkey), the indicator viewport closes and the main window is shown, raised, and focused directly on the `Editing` screen.

**F9's role changes**: it no longer *starts* a recording (that's now a button click on the Project screen) — it's a global stop-recording shortcut, active only while `Recording` is in progress. It has no effect in other states.

## State machine

```
Project (FPS selector + "Tela Inteira" / "Selecionar Área" buttons)
  --[Tela Inteira]-----------------------------> Recording
  --[Selecionar Área]--> SelectingArea --[drag released]--> Recording
Recording --[stop: F9 or indicator button]--> Editing
Editing --[Exportar, choose path]--> Exporting --[done]--> Done
Editing --["Nova gravação"]--> Project (discards current session)
Done --[implicit / "Nova gravação"]--> Project
```

`AppState` gains `Project` (replacing today's `Selecting`) and `SelectingArea`; `Recording`, `Editing`, `Exporting`, `Done` keep their current shape with additions noted below.

## Screens

### Project (replaces today's `Selecting`)

- Title "Gravar tela" + one-line explanation.
- Static placeholder preview (dark box, monitor icon, "Pronto para gravar" — not a live preview; avoids capturing before the user has committed to a mode).
- FPS selector: fixed presets **8 / 12 / 15 / 20**, default 20. Replaces the current hardcoded `CAPTURE_FPS = 10` constant with app state that flows into `start_capture`.
- Two buttons, side by side: **"Tela Inteira"** and **"Selecionar Área"**.

### Selection overlay (`SelectingArea`, transient)

- On "Selecionar Área": main window hides, overlay viewport opens spanning the bounding box of all monitors (via `xcap::Monitor::all()`), background darkened/semi-transparent.
- User drags a rectangle; it's drawn live with a highlighted border as the pointer moves.
- On release: overlay closes, the dragged rectangle (in global desktop coordinates) becomes the `capture::Region`, and the state moves straight to `Recording` — no separate confirm step (matches the approved Section B flow).
- Degenerate drags (zero width/height, e.g. a stray click) are ignored — the overlay stays open.
- **Known limitation, carried over from the MVP:** `capture::start_capture` resolves a single `xcap::Monitor` from the region's top-left origin and captures using coordinates local to that monitor. A drag that crosses monitor boundaries is not rejected, but frames will be captured relative to whichever monitor contains the region's origin — not specially handled or fixed by this redesign.

### Recording indicator (`Recording`, transient viewport + existing capture thread)

- Small always-on-top viewport, positioned in a screen corner (e.g. top-right of the captured region's monitor).
- Shows `● REC · MM:SS · N frames`, updating every frame from the same `Receiver<Frame>` count and elapsed time already tracked by `capture::start_capture`.
- A stop control (button) alongside the text; F9 does the same thing globally.
- On stop: capture thread joins (same logic as today — a mid-recording `CaptureError` only stops future frames, already-sent frames are kept), indicator viewport closes, main window un-hides, raises, and focuses on `Editing`.

### Editing (full relayout)

Top bar: app name (left), **"Exportar"** button (right). *No "Abrir projeto"* — project save/load stays out of scope; "back" is only via "← Nova gravação".

Second row: "← Nova gravação" (left, returns to `Project`, discards the in-memory session — no confirmation dialog, matching the MVP's "single-session flow" constraint) and "N frames" count (right).

Body, two columns:

- **Preview (left/center, large):** shows the selected frame. While "Prévia" is active, cycles frames in a loop using their real captured `timestamp_ms` deltas for playback speed (falls back to a fixed interval if consecutive timestamps are equal/decreasing, which capture timing jitter can occasionally produce).
- **Tool panel (right sidebar, fixed width):**
  - **"▷ Prévia"** button (toggles play/pause of the loop preview) + a replay/loop icon.
  - Tool selector — one active at a time, visually highlighted: **Selecionar** (default) / **Recortar** / **Texto** / **Blur**.
  - Contextual sub-panel, switches with the active tool:
    - *Selecionar*: "FRAME X DE N", **Duplicar** button, delete button. Reorder (**◀**/**▶**) also lives here, next to delete — same mechanism as today, just relocated into this panel.
    - *Recortar*: existing drag-to-crop behavior (unchanged logic, relocated under this tool), "Cancelar corte" toggle.
    - *Texto*: text input field; clicking the preview places the text at that point; "Aplicar" bakes it into all frames, "Cancelar" discards.
    - *Blur*: drag a region on the preview (reuses the crop drag math); intensity slider (blur sigma); "Aplicar"/"Cancelar".
  - **"✂ Exportar GIF"** button, pinned to the bottom of the sidebar (same action as the top bar's "Exportar").

Bottom: horizontal filmstrip, one thumbnail per frame with index + time label, scrollable. Click selects the frame shown in the preview and in the "Selecionar" panel. (Drag-to-reorder deferred per the approved decision — reorder stays button-driven.)

### Exporting / Done

Unchanged in behavior from the current MVP (progress bar, then "Salvo em: ..."), restyled to match the new visual language (dark theme, consistent button/typography treatment with the rest of the redesigned screens).

## Data model additions (`editor` crate)

Same pattern as the existing `crop` — pure operations on `FrameList` that apply to every frame, fully unit-testable without a display:

- **`duplicate(&mut self, index: usize) -> Result<(), EditorError>`** — clones the frame at `index`, inserts the clone immediately after it. `IndexOutOfBounds` on invalid index.
- **`blur(&mut self, rect: CropRect, sigma: f32) -> Result<(), EditorError>`** — applies Gaussian blur (via `image::imageops::blur`, already a transitive capability of the existing `image` dependency — no new crate) to the sub-region `rect` in every frame, in place. Reuses the existing `CropRect` type for the region rather than introducing a parallel type. Bounds validation identical to `crop`'s (`InvalidCropRect` on an out-of-bounds or zero-size rect).
- **`add_text(&mut self, position: (u32, u32), text: String, font_size: f32, color: [u8; 4]) -> Result<(), EditorError>`** — burns `text` into every frame at `position` (top-left anchor), via `imageproc::drawing::draw_text_mut` with an `ab_glyph` font loaded from bytes embedded with `include_bytes!` (a permissively-licensed font shipped in the repo, e.g. DejaVu Sans — OFL/public-domain-compatible, redistributable). New error variant **`EditorError::EmptyText`** when `text` is empty. Text drawn partially or fully outside the frame bounds is allowed (clipped, not an error) — simpler than pre-validating placement against every frame's dimensions.

New dependencies for `editor`: `imageproc`, `ab_glyph`, plus one embedded font asset file.

## App-level additions (`app` crate)

- `AppState::Project { fps: u32 }` (replaces `Selecting`), `AppState::SelectingArea { fps: u32 }` (new).
- `Recording` gains the fields needed to render the indicator (already has everything needed except a `start_time`/frame counter, which it can derive from `rx`'s received count and an `Instant` captured at transition time).
- `Editing` gains `active_tool: Tool` (enum `Selecionar | Recortar | Texto | Blur`) and per-tool transient UI state (pending crop drag, pending text position + input buffer, pending blur drag + sigma) — mirrors how `EditorScreen` already tracks `cropping`/`drag_start`.
- New pure, unit-tested helper functions in `app`, following the existing `crop_rect_from_drag` / `selection_after_delete` pattern:
  - `text_position_from_click(click: (f32, f32), displayed_size: (f32, f32), image_pixel_size: (u32, u32)) -> (u32, u32)` — same scale-aware conversion as `crop_rect_from_drag`, for a single point instead of a rect.
  - `selection_after_duplicate(selected: usize, duplicated_at: usize) -> usize` — the new copy (at `duplicated_at + 1`) becomes selected.
  - Blur's drag reuses `crop_rect_from_drag` directly (identical math) — no new function needed.
- "Tela Inteira" computes a `capture::Region` covering the monitor containing the main window's current position (via `xcap::Monitor::from_point` on the window's center, same lookup `capture` already does internally) — no changes needed to the `capture` crate itself.

## Testing strategy

- `editor`: unit tests for `duplicate` (length +1, cloned values match, position correct), `blur` (pixel values in the region change, e.g. a sharp two-color edge becomes intermediate values, across every frame), `add_text` (pixels in the text's bounding area differ from the untouched background, across every frame), `EditorError::EmptyText` on empty input — same style as the existing `crop` tests.
- `app`: `text_position_from_click` and `selection_after_duplicate` get the same table-style unit tests as `crop_rect_from_drag`/`selection_after_delete`.
- `capture`/window-architecture pieces (overlay drag, indicator viewport, window hide/show/focus) stay manual-verification only, same rationale as today (needs a live X11 display) — the manual checklist in `README.md` gets rewritten for the new flow: Project → Tela Inteira **and** Selecionar Área → indicator visible and accurate → stop via indicator button **and** via F9 → editor shows result immediately focused → each tool (Selecionar/Recortar/Texto/Blur) exercised → Prévia plays back → Exportar → resulting GIF reflects all edits.

## Migration notes (relative to the current MVP)

- `AppState::Selecting` is removed/renamed to `Project`; the "the app window is the region" mechanic is removed entirely, along with `selection::region_from_window_rect` (superseded by the overlay's own coordinate math).
- `editor_screen.rs` is substantially rewritten for the new layout and tool-panel structure; the existing crop drag math (`crop_tool::crop_rect_from_drag`) is reused as-is for both Recortar and Blur.
- `export_screen.rs` (`start_export`/`ExportJob`) is unchanged.
- `main.rs`'s state machine grows two states and gains multi-viewport orchestration; F9's meaning narrows from "toggle start/stop" to "stop only."

