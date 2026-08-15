/// Converts a click on the displayed preview image into a pixel position
/// in the original image, accounting for display scale (mirrors
/// `crop_tool::crop_rect_from_drag`'s scale math for a single point).
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
