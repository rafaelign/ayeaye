use editor::{Frame, FrameList};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub struct ExportJob {
    pub progress: Arc<Mutex<(usize, usize)>>,
    pub handle: JoinHandle<Result<(), encoder::EncodeError>>,
}

pub fn start_export(frames: &FrameList, output_path: PathBuf) -> ExportJob {
    let progress = Arc::new(Mutex::new((0usize, frames.len())));
    let progress_for_thread = progress.clone();
    let owned_frames: Vec<Frame> = frames.frames().to_vec();

    let handle = std::thread::spawn(move || {
        encoder::encode_gif(&owned_frames, output_path, move |current, total| {
            *progress_for_thread.lock().unwrap() = (current, total);
        })
    });

    ExportJob { progress, handle }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn start_export_produces_a_gif_file_and_reports_full_progress() {
        let frames = FrameList::new(vec![
            Frame { image: RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255])), timestamp_ms: 0 },
            Frame { image: RgbaImage::from_pixel(2, 2, image::Rgba([0, 255, 0, 255])), timestamp_ms: 100 },
        ]);
        let output = std::env::temp_dir().join("export_screen_test.gif");

        let job = start_export(&frames, output.clone());
        job.handle.join().expect("export thread panicked").expect("export failed");

        let (current, total) = *job.progress.lock().unwrap();
        assert_eq!((current, total), (2, 2));
        assert!(output.exists());

        std::fs::remove_file(&output).ok();
    }
}
