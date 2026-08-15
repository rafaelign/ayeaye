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
                        ui.colored_label(egui::Color32::RED, egui::RichText::new("REC").strong());
                        let mins = elapsed_secs / 60;
                        let secs = elapsed_secs % 60;
                        ui.label(format!("{mins:02}:{secs:02} · {frame_count} frames"));
                        if ui.button("Parar").clicked() {
                            stop_clicked = true;
                        }
                    });
                });
        },
    );
    stop_clicked
}
