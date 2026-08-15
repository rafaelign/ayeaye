use eframe::egui;
use editor::FrameList;
use image::RgbaImage;

use crate::strings::Strings;

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Selecionar,
    Recortar,
    Blur,
    Texto,
}

pub enum EditorAction {
    Delete(usize),
    Reorder(usize, usize),
    Duplicate(usize),
    Crop(editor::CropRect),
    Blur(editor::CropRect, f32),
    AddText { position: (u32, u32), text: String, font_size: f32 },
}

/// Filmstrip thumbnails are shrunk to at most this many pixels on their
/// longer side before becoming textures. 2x the ~80px on-screen thumbnail
/// size keeps them crisp on high-DPI displays while staying far cheaper
/// than uploading full-resolution frames just to show them tiny.
pub const THUMBNAIL_MAX_DIM: u32 = 160;

pub struct EditorScreen {
    thumbnails: Vec<egui::TextureHandle>,
    /// The single full-resolution texture for whichever frame is currently
    /// shown in the big preview, and the index it belongs to. Recreated
    /// only when the displayed frame changes, instead of eagerly loading a
    /// full-res texture for every frame up front — with hundreds of
    /// captured frames, doing that synchronously on the UI thread is slow
    /// enough to feel like the app froze.
    preview: Option<(usize, egui::TextureHandle)>,
    pub selected: usize,
    tool: Tool,
    crop_drag_start: Option<egui::Pos2>,
    blur_drag_start: Option<egui::Pos2>,
    blur_sigma: f32,
    text_input: String,
    playing: bool,
    play_started_at: Option<std::time::Instant>,
    /// Measured height of the toolbar's actual (possibly wrapped) content,
    /// from the previous frame — see the comment where this is used.
    toolbar_height: f32,
}

impl EditorScreen {
    /// Uploads already-resized thumbnail images as GPU textures (see
    /// `processing::start_initial_processing`/`start_edit_processing`,
    /// which build them off the UI thread). Must run on the main thread —
    /// texture uploads go through the egui/eframe render backend.
    pub fn from_thumbnail_images(ctx: &egui::Context, thumbnails: &[RgbaImage]) -> Self {
        let thumbnails = thumbnails
            .iter()
            .enumerate()
            .map(|(i, thumb)| {
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [thumb.width() as usize, thumb.height() as usize],
                    thumb.as_raw(),
                );
                ctx.load_texture(format!("thumb-{i}"), color_image, egui::TextureOptions::default())
            })
            .collect();
        Self {
            thumbnails,
            preview: None,
            selected: 0,
            tool: Tool::Selecionar,
            crop_drag_start: None,
            blur_drag_start: None,
            blur_sigma: 4.0,
            text_input: String::new(),
            playing: false,
            play_started_at: None,
            toolbar_height: 52.0,
        }
    }

    /// Renders the editor body as four horizontal bands, top to bottom:
    /// a toolbar (tool picker + the active tool's controls), the large
    /// preview, the frame timeline, and a status bar. Returns an action
    /// once the user triggers one via the toolbar.
    ///
    /// The toolbar, timeline, and status bar are `egui::Panel`s (claimed
    /// *before* the preview is laid out) rather than plain
    /// `ui.horizontal`/`ui.vertical` nesting — a `Panel` reserves a fixed
    /// slice of space up front, so the remaining `ui.available_size()` for
    /// the preview is a real, stable number instead of a value that
    /// depends on the preview's own content (which previously collapsed to
    /// the frame's tiny native pixel size).
    pub fn show(&mut self, ui: &mut egui::Ui, frames: &FrameList, strings: &Strings) -> Option<EditorAction> {
        let mut action = None;

        // Status bar: claimed first among the bottom panels, so it ends up
        // at the very bottom edge — a contextual hint for the active tool
        // on the left, total frame count on the right.
        egui::Panel::bottom("editor_status_bar").exact_size(28.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                let hint = match self.tool {
                    Tool::Selecionar => strings.frame_x_of_n(self.selected + 1, frames.len()),
                    Tool::Recortar => strings.hint_crop.to_string(),
                    Tool::Blur => strings.hint_blur.to_string(),
                    Tool::Texto => strings.hint_text.to_string(),
                };
                ui.label(hint);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{} frames", frames.len()));
                });
            });
        });

        // Timeline: claimed second, so it sits directly above the status bar.
        egui::Panel::bottom("editor_filmstrip").exact_size(110.0).show(ui, |ui| {
            ui.add_space(4.0);
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (i, (texture, frame)) in self.thumbnails.iter().zip(frames.frames()).enumerate() {
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
        });

        // Toolbar: a single row above the preview — tool picker, then the
        // active tool's own controls. Wrapped, not a plain horizontal row,
        // so a narrow window reflows to a second line instead of the tool's
        // controls overflowing past the window edge. Wrapped content can
        // need one or two rows depending on window width, so — same
        // measure-this-frame-use-next-frame technique as the Project
        // screen's centering — the panel's height tracks the content's
        // last-measured height instead of a fixed guess that clips a
        // wrapped second row.
        egui::Panel::top("editor_toolbar").exact_size(self.toolbar_height).show(ui, |ui| {
            ui.add_space(4.0);
            let content = ui.horizontal_wrapped(|ui| {
                if ui.button(if self.playing { strings.pause_button } else { strings.play_button }).clicked() {
                    self.playing = !self.playing;
                    self.play_started_at = if self.playing { Some(std::time::Instant::now()) } else { None };
                }
                ui.separator();
                if ui.selectable_label(self.tool == Tool::Selecionar, strings.tool_select).clicked() {
                    self.tool = Tool::Selecionar;
                }
                if ui.selectable_label(self.tool == Tool::Recortar, strings.tool_crop).clicked() {
                    self.tool = Tool::Recortar;
                    self.crop_drag_start = None;
                }
                if ui.selectable_label(self.tool == Tool::Blur, strings.tool_blur).clicked() {
                    self.tool = Tool::Blur;
                    self.blur_drag_start = None;
                }
                if ui.selectable_label(self.tool == Tool::Texto, strings.tool_text).clicked() {
                    self.tool = Tool::Texto;
                }
                ui.separator();

                match self.tool {
                    Tool::Selecionar => {
                        if ui.button(strings.duplicate_button).clicked() {
                            action = Some(EditorAction::Duplicate(self.selected));
                        }
                        if ui.add_enabled(self.selected > 0, egui::Button::new(strings.move_left_button)).clicked() {
                            action = Some(EditorAction::Reorder(self.selected, self.selected - 1));
                        }
                        if ui.add_enabled(self.selected + 1 < frames.len(), egui::Button::new(strings.move_right_button)).clicked() {
                            action = Some(EditorAction::Reorder(self.selected, self.selected + 1));
                        }
                        if ui.add_enabled(!frames.is_empty(), egui::Button::new(strings.delete_frame_button)).clicked() {
                            action = Some(EditorAction::Delete(self.selected));
                        }
                    }
                    Tool::Recortar => {}
                    Tool::Blur => {
                        ui.add(egui::Slider::new(&mut self.blur_sigma, 1.0..=20.0).text(strings.intensity_label));
                    }
                    Tool::Texto => {
                        ui.text_edit_singleline(&mut self.text_input);
                    }
                }
            });
            self.toolbar_height = content.response.rect.height() + 12.0;
        });

        // Whatever's left of `ui` after the panels above is the preview
        // area — its `available_size()` is now a real, non-circular value.
        let display_index = match self.play_started_at {
            Some(start) => {
                let timestamps: Vec<u64> = frames.frames().iter().map(|f| f.timestamp_ms).collect();
                frame_index_at(&timestamps, start.elapsed().as_millis() as u64)
            }
            None => self.selected,
        };
        if self.preview.as_ref().map(|(i, _)| *i) != Some(display_index) {
            if let Some(frame) = frames.frames().get(display_index) {
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [frame.image.width() as usize, frame.image.height() as usize],
                    frame.image.as_raw(),
                );
                let texture = ui.ctx().load_texture("preview", color_image, egui::TextureOptions::default());
                self.preview = Some((display_index, texture));
            }
        }

        if let Some((preview_index, texture)) = &self.preview {
            let sense = match self.tool {
                Tool::Recortar | Tool::Blur => egui::Sense::drag(),
                Tool::Texto => egui::Sense::click(),
                Tool::Selecionar => egui::Sense::hover(),
            };
            let available = ui.available_size();

            // `fit_to_exact_size` only constrains how large the image is
            // allowed to render — the widget's own rect still shrinks to
            // that (aspect-preserving) size, so unless the frame's aspect
            // ratio happens to exactly match the available area, egui
            // leaves it pinned in the corner rather than centering it.
            // Compute the true displayed size ourselves and pad both axes
            // so the image sits centered in the middle of the preview area.
            let native = frames
                .frames()
                .get(*preview_index)
                .map(|f| egui::vec2(f.image.width() as f32, f.image.height() as f32))
                .unwrap_or(available);
            let scale = (available.x / native.x.max(1.0)).min(available.y / native.y.max(1.0));
            let display_size = native * scale;
            let pad_x = ((available.x - display_size.x) / 2.0).max(0.0);
            let pad_y = ((available.y - display_size.y) / 2.0).max(0.0);

            let image_response = ui
                .horizontal(|ui| {
                    ui.add_space(pad_x);
                    ui.vertical(|ui| {
                        ui.add_space(pad_y);
                        ui.add(egui::Image::new(texture).fit_to_exact_size(display_size).sense(sense))
                    })
                    .inner
                })
                .inner;

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
                    action = Some(EditorAction::AddText {
                        position,
                        text: self.text_input.clone(),
                        font_size: 24.0,
                    });
                    self.text_input.clear();
                }
            }
        }

        action
    }

    pub fn apply_delete(&mut self, index: usize) {
        let _ = self.thumbnails.remove(index); // dropping frees the GPU texture
        self.selected = selection_after_delete(self.selected, index, self.thumbnails.len());
        self.preview = None; // frame indices shifted — force a fresh preview texture
    }

    pub fn apply_reorder(&mut self, from: usize, to: usize) {
        let texture = self.thumbnails.remove(from);
        self.thumbnails.insert(to, texture);
        self.selected = to;
        self.preview = None; // frame indices shifted — force a fresh preview texture
    }

    /// Duplicate is cheap enough to stay synchronous: `TextureHandle` is
    /// `Clone` (it's a ref-counted handle into the GPU texture manager), so
    /// reusing the existing thumbnail texture for the new copy costs no
    /// resize or re-upload at all — unlike crop/blur/text, which change
    /// every frame's pixels and genuinely need the background job.
    pub fn apply_duplicate(&mut self, index: usize) {
        let cloned = self.thumbnails[index].clone();
        self.thumbnails.insert(index + 1, cloned);
        self.selected = selection_after_duplicate(index);
        self.preview = None; // frame indices shifted — force a fresh preview texture
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

/// Scales `source` down so its longer side is at most `max_dim`, preserving
/// aspect ratio. Used to shrink captured frames before turning them into
/// filmstrip thumbnail textures.
pub fn thumbnail_dimensions(source: (u32, u32), max_dim: u32) -> (u32, u32) {
    let (w, h) = source;
    if w == 0 || h == 0 {
        return (1, 1);
    }
    if w >= h {
        let new_w = max_dim.min(w).max(1);
        let new_h = ((h as f32) * (new_w as f32 / w as f32)).round().max(1.0) as u32;
        (new_w, new_h)
    } else {
        let new_h = max_dim.min(h).max(1);
        let new_w = ((w as f32) * (new_h as f32 / h as f32)).round().max(1.0) as u32;
        (new_w, new_h)
    }
}

/// Given ascending frame timestamps (as captured) and elapsed time since
/// playback started, returns which frame index should be showing, looping
/// back to the start once elapsed passes the last timestamp.
pub fn frame_index_at(timestamps_ms: &[u64], elapsed_ms: u64) -> usize {
    if timestamps_ms.is_empty() {
        return 0;
    }
    let total = timestamps_ms.last().copied().unwrap_or(0).max(1);
    let looped = elapsed_ms % total;
    timestamps_ms.iter().rposition(|&t| t <= looped).unwrap_or(0)
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

    #[test]
    fn thumbnail_dimensions_shrinks_a_wide_image_by_its_width() {
        assert_eq!(thumbnail_dimensions((1920, 1080), 160), (160, 90));
    }

    #[test]
    fn thumbnail_dimensions_shrinks_a_tall_image_by_its_height() {
        assert_eq!(thumbnail_dimensions((1080, 1920), 160), (90, 160));
    }

    #[test]
    fn thumbnail_dimensions_leaves_a_small_image_unshrunk() {
        assert_eq!(thumbnail_dimensions((100, 50), 160), (100, 50));
    }
}
