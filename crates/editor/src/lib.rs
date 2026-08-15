use ab_glyph::{FontRef, PxScale};
use image::RgbaImage;
use imageproc::drawing::draw_text_mut;

static FONT_BYTES: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");

#[derive(Clone)]
pub struct Frame {
    pub image: RgbaImage,
    pub timestamp_ms: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditorError {
    IndexOutOfBounds,
    InvalidCropRect,
    EmptyText,
}

impl std::fmt::Display for EditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditorError::IndexOutOfBounds => write!(f, "frame index out of bounds"),
            EditorError::InvalidCropRect => write!(f, "crop rect is invalid for this frame"),
            EditorError::EmptyText => write!(f, "text must not be empty"),
        }
    }
}

impl std::error::Error for EditorError {}

pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub struct FrameList {
    frames: Vec<Frame>,
}

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

    pub fn duplicate(&mut self, index: usize) -> Result<(), EditorError> {
        if index >= self.frames.len() {
            return Err(EditorError::IndexOutOfBounds);
        }
        let clone = self.frames[index].clone();
        self.frames.insert(index + 1, clone);
        Ok(())
    }

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
}
