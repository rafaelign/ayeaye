/// Converts a drag on the full-desktop selection overlay (whose own
/// viewport sits at `viewport_origin` in global desktop coordinates) into
/// a `capture::Region` in those same global coordinates.
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

use eframe::egui;

/// Renders the full-desktop selection overlay as its own viewport,
/// spanning `bounds` (the union of every monitor, in desktop coordinates —
/// see `capture::virtual_screen_bounds`). `drag_start` is owned by the
/// caller (`AppState::SelectingArea`) so it survives across frames, the
/// same pattern `crop_tool`'s callers use. Returns the selected region
/// once the user releases a non-degenerate drag; the caller stops calling
/// this function once that happens, which closes the overlay.
///
/// `backdrop` is a real screenshot of every monitor (captured once, via
/// `capture::snapshot_monitors`, when entering `SelectingArea`), drawn
/// behind the dimming so the user can always see the actual desktop —
/// this doesn't depend on the window manager honoring the viewport's
/// `with_transparent(true)`, which isn't reliably the case on every setup.
pub fn show(
    ctx: &egui::Context,
    bounds: capture::Region,
    backdrop: &[(capture::Region, egui::TextureHandle)],
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

            for (region, texture) in backdrop {
                let local_min = egui::pos2((region.x - bounds.x) as f32, (region.y - bounds.y) as f32);
                let local_max = local_min + egui::vec2(region.width as f32, region.height as f32);
                ui.painter().image(
                    texture.id(),
                    egui::Rect::from_min_max(local_min, local_max),
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }

            if response.drag_started() {
                *drag_start = ui.input(|i| i.pointer.interact_pos()).map(|p| (p.x, p.y));
            }

            if let (Some(start), Some(current)) = (*drag_start, ui.input(|i| i.pointer.interact_pos())) {
                // "Spotlight": dim everything outside the dragged rect, leave
                // the selection itself showing the real screenshot clearly.
                let sel = egui::Rect::from_two_pos(egui::pos2(start.0, start.1), current);
                let full = response.rect;
                let dim = egui::Color32::from_black_alpha(140);
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(full.min, egui::pos2(full.max.x, sel.min.y)),
                    0.0,
                    dim,
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(egui::pos2(full.min.x, sel.max.y), full.max),
                    0.0,
                    dim,
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(egui::pos2(full.min.x, sel.min.y), egui::pos2(sel.min.x, sel.max.y)),
                    0.0,
                    dim,
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(egui::pos2(sel.max.x, sel.min.y), egui::pos2(full.max.x, sel.max.y)),
                    0.0,
                    dim,
                );
                ui.painter().rect_stroke(sel, 0.0, egui::Stroke::new(2.0, egui::Color32::YELLOW), egui::StrokeKind::Outside);
            } else {
                // No drag yet — a light tint signals "you're selecting an
                // area" without hiding the screen underneath.
                ui.painter().rect_filled(response.rect, 0.0, egui::Color32::from_black_alpha(25));
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
