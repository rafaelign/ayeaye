use eframe::egui;

use crate::strings::Strings;
use crate::theme;

pub enum ProjectAction {
    StartFullScreen,
    StartAreaSelection,
}

pub struct ProjectScreen {
    pub fps: u32,
    /// Measured width of the FPS/mode rows' actual content, from the
    /// previous frame — see the comment above where these are used.
    fps_row_width: f32,
    mode_row_width: f32,
}

impl Default for ProjectScreen {
    fn default() -> Self {
        Self { fps: 12, fps_row_width: 0.0, mode_row_width: 0.0 }
    }
}

impl ProjectScreen {
    const FPS_PRESETS: [u32; 4] = [8, 12, 15, 20];

    pub fn show(&mut self, ui: &mut egui::Ui, logo: &egui::TextureHandle, strings: &Strings) -> Option<ProjectAction> {
        let mut action = None;

        // Claimed first, like the editor's own status bar, so the
        // centered content above sizes itself against the space that's
        // actually left over instead of overlapping this footer.
        egui::Panel::bottom("project_footer").exact_size(24.0).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(4.0);
                // `Hyperlink`/`hyperlink_to` always paint with
                // `visuals().hyperlink_color` — an explicit `.color()` on
                // the RichText itself is ignored — so the only way to keep
                // this link visually quiet (not the same bright accent as
                // every other link in the app) is to override the color
                // for just this one widget via a scoped style.
                ui.scope(|ui| {
                    ui.visuals_mut().hyperlink_color = ui.visuals().weak_text_color();
                    ui.hyperlink_to(egui::RichText::new(strings.donation_link).size(12.0), "https://ko-fi.com/H2H010PKL5");
                });
            });
        });

        // The placeholder box scales with the window (65% of the available
        // width, within sane bounds) and keeps a 16:9 aspect ratio, and the
        // heading grows with it too — so the screen actually fills a large
        // window instead of staying a small fixed-size island surrounded by
        // dead space.
        let box_width = (ui.available_width() * 0.65).clamp(280.0, 900.0);
        let box_height = (box_width * 9.0 / 16.0).clamp(160.0, 560.0);
        let heading_size = (ui.available_width() / 25.0).clamp(28.0, 56.0);
        let logo_size = (heading_size * 2.4).clamp(64.0, 128.0);
        let name_size = (heading_size * 0.6).clamp(18.0, 26.0);

        // vertical_centered only centers each widget horizontally — without
        // this top padding the whole block stays pinned to the top on a
        // large window, with a huge empty gap below it.
        let estimated_content_height =
            logo_size + 2.0 + name_size * 1.3 + 12.0 + heading_size * 1.5 + 24.0 + 16.0 * 3.0 + box_height + 40.0 + 32.0;
        let top_padding = ((ui.available_height() - estimated_content_height) / 2.0).max(0.0);

        ui.vertical_centered(|ui| {
            ui.add_space(top_padding);
            ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(logo_size, logo_size)));
            ui.add_space(2.0);
            ui.label(egui::RichText::new("AyeAye").size(name_size).strong().color(theme::ACCENT));
            ui.add_space(12.0);
            ui.label(egui::RichText::new(strings.record_screen_heading).size(heading_size).color(ui.visuals().text_color()));
            ui.label(strings.record_screen_subtitle);
            ui.add_space(16.0);

            let (rect, _) = ui.allocate_exact_size(egui::vec2(box_width, box_height), egui::Sense::hover());
            ui.painter().rect_filled(rect, 8.0, egui::Color32::from_gray(40));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                strings.ready_to_record,
                egui::FontId::proportional((heading_size / 1.8).max(14.0)),
                egui::Color32::GRAY,
            );

            // `ui.horizontal` always claims the row's full available width
            // for its own allocation (it sets `initial_size.x` to
            // `available_size_before_wrap().x` internally) and `main_align`
            // only aligns content *within* an already-known size (e.g. text
            // inside a button) — neither actually centers a row of
            // variable-width sibling widgets as a group. So: measure the
            // row's real content width on one frame (via a nested
            // `ui.horizontal`, whose returned rect *does* shrink to its
            // content), and pad the *next* frame by that measurement. The
            // app repaints continuously, so the one-frame lag is invisible.
            ui.add_space(16.0);
            let fps_pad = ((ui.available_width() - self.fps_row_width) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(fps_pad);
                let content = ui.horizontal(|ui| {
                    ui.label("FPS");
                    for preset in Self::FPS_PRESETS {
                        if ui.selectable_label(self.fps == preset, preset.to_string()).clicked() {
                            self.fps = preset;
                        }
                    }
                });
                self.fps_row_width = content.response.rect.width();
            });

            ui.add_space(16.0);
            let mode_pad = ((ui.available_width() - self.mode_row_width) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(mode_pad);
                let content = ui.horizontal(|ui| {
                    if ui.button(strings.full_screen_button).clicked() {
                        action = Some(ProjectAction::StartFullScreen);
                    }
                    if ui.button(strings.select_area_button).clicked() {
                        action = Some(ProjectAction::StartAreaSelection);
                    }
                });
                self.mode_row_width = content.response.rect.width();
            });
        });
        action
    }
}
