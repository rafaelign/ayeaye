use editor::FrameList;
use image::RgbaImage;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// A heavy `FrameList` edit to apply before rebuilding the editor's
/// thumbnails — see `start_edit_processing`.
pub enum EditOp {
    Crop(editor::CropRect),
    Blur(editor::CropRect, f32),
    AddText { position: (u32, u32), text: String, font_size: f32 },
}

/// The result of a background processing job: the (possibly edited)
/// frames, plus their filmstrip thumbnails already resized and ready to
/// become GPU textures on the main thread via
/// `EditorScreen::from_thumbnail_images`.
pub struct PreparedFrames {
    pub frames: FrameList,
    pub thumbnails: Vec<RgbaImage>,
}

pub struct ProcessingJob {
    pub progress: Arc<Mutex<(usize, usize)>>,
    pub handle: JoinHandle<PreparedFrames>,
}

/// Starts building thumbnails for `frames` as-is — used right after a
/// recording stops, so the (potentially long) list of captured frames gets
/// resized off the UI thread instead of freezing the app the moment
/// recording ends.
pub fn start_initial_processing(frames: FrameList) -> ProcessingJob {
    start(frames, None)
}

/// Applies `op` to `frames` and then builds thumbnails, all off the UI
/// thread — crop/blur/add_text each touch every frame's full-resolution
/// pixels, which is slow enough on a long recording to otherwise freeze
/// the app for the duration of the edit.
pub fn start_edit_processing(frames: FrameList, op: EditOp) -> ProcessingJob {
    start(frames, Some(op))
}

fn start(mut frames: FrameList, op: Option<EditOp>) -> ProcessingJob {
    let progress = Arc::new(Mutex::new((0usize, frames.len())));
    let progress_for_thread = progress.clone();
    let handle = std::thread::spawn(move || {
        match op {
            None => {}
            Some(EditOp::Crop(rect)) => {
                frames.crop(rect).expect("crop rect came from the UI, must be valid");
            }
            Some(EditOp::Blur(rect, sigma)) => {
                frames.blur(rect, sigma).expect("blur rect came from the UI, must be valid");
            }
            Some(EditOp::AddText { position, text, font_size }) => {
                frames
                    .add_text(position, text, font_size, [255, 255, 255, 255])
                    .expect("text came from the UI, must be non-empty");
            }
        }

        let total = frames.len();
        let thumbnails = frames
            .frames()
            .iter()
            .enumerate()
            .map(|(i, frame)| {
                let (w, h) = crate::editor_screen::thumbnail_dimensions(
                    (frame.image.width(), frame.image.height()),
                    crate::editor_screen::THUMBNAIL_MAX_DIM,
                );
                let thumb = image::imageops::resize(&frame.image, w, h, image::imageops::FilterType::Triangle);
                *progress_for_thread.lock().unwrap() = (i + 1, total);
                thumb
            })
            .collect();

        PreparedFrames { frames, thumbnails }
    });
    ProcessingJob { progress, handle }
}
