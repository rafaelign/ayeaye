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
