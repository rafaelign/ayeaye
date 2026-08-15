use eframe::egui;

/// Accent used for selection, hover outlines, and primary actions —
/// a warm amber, like the eyeshine of a nocturnal aye-aye.
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(232, 163, 61);

/// Applies AyeAye's visual identity once, at startup: a warm near-black
/// surface (rather than egui's default neutral gray), one amber accent for
/// anything selected/primary, and generous rounded corners and padding so
/// buttons read as tappable, friendly surfaces instead of flat rectangles.
pub fn apply(ctx: &egui::Context) {
    let bg = egui::Color32::from_rgb(20, 18, 15);
    let surface = egui::Color32::from_rgb(33, 29, 23);
    let surface_hover = egui::Color32::from_rgb(44, 38, 29);
    let accent_active = egui::Color32::from_rgb(201, 138, 46);
    let text = egui::Color32::from_rgb(242, 237, 228);
    let text_muted = egui::Color32::from_rgb(167, 158, 142);
    let corner_radius: egui::CornerRadius = 10.into();

    ctx.all_styles_mut(|style| {
        let visuals = &mut style.visuals;
        visuals.dark_mode = true;
        visuals.panel_fill = bg;
        visuals.window_fill = bg;
        visuals.extreme_bg_color = egui::Color32::from_rgb(14, 12, 10);
        visuals.faint_bg_color = surface;
        visuals.weak_text_color = Some(text_muted);
        visuals.hyperlink_color = ACCENT;

        visuals.widgets.noninteractive.bg_fill = bg;
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text);
        visuals.widgets.noninteractive.corner_radius = corner_radius;

        visuals.widgets.inactive.bg_fill = surface;
        visuals.widgets.inactive.weak_bg_fill = surface;
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text);
        visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.inactive.corner_radius = corner_radius;

        visuals.widgets.hovered.bg_fill = surface_hover;
        visuals.widgets.hovered.weak_bg_fill = surface_hover;
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, ACCENT);
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT);
        visuals.widgets.hovered.corner_radius = corner_radius;

        visuals.widgets.active.bg_fill = accent_active;
        visuals.widgets.active.weak_bg_fill = accent_active;
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, bg);
        visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.active.corner_radius = corner_radius;

        // Selected state (FPS preset, active tool, chosen filmstrip frame).
        visuals.selection.bg_fill = ACCENT;
        visuals.selection.stroke = egui::Stroke::new(1.0, bg);

        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    });
}
