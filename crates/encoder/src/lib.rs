use editor::Frame;
use gifski::progress::ProgressReporter;
use std::fs::File;
use std::path::Path;

#[derive(Debug)]
pub enum EncodeError {
    NoFrames,
    Encoding(String),
    Io(std::io::Error),
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::NoFrames => write!(f, "cannot encode an empty frame list"),
            EncodeError::Encoding(msg) => write!(f, "gif encoding failed: {msg}"),
            EncodeError::Io(err) => write!(f, "could not write gif file: {err}"),
        }
    }
}

impl std::error::Error for EncodeError {}

struct CallbackProgress<F: FnMut(usize, usize)> {
    total: usize,
    current: usize,
    callback: F,
}

impl<F: FnMut(usize, usize) + Send> ProgressReporter for CallbackProgress<F> {
    fn increase(&mut self) -> bool {
        self.current += 1;
        (self.callback)(self.current, self.total);
        true
    }

    fn done(&mut self, _msg: &str) {}
}

pub fn encode_gif<P: AsRef<Path>>(
    frames: &[Frame],
    output_path: P,
    progress: impl FnMut(usize, usize) + Send + 'static,
) -> Result<(), EncodeError> {
    if frames.is_empty() {
        return Err(EncodeError::NoFrames);
    }

    let settings = gifski::Settings::default();
    let (collector, writer) =
        gifski::new(settings).map_err(|e| EncodeError::Encoding(e.to_string()))?;

    let frames_owned: Vec<Frame> = frames.to_vec();

    let collect_handle = std::thread::spawn(move || -> Result<(), EncodeError> {
        for (i, frame) in frames_owned.iter().enumerate() {
            let pixels: Vec<rgb::RGBA8> = frame
                .image
                .pixels()
                .map(|p| rgb::RGBA8::new(p[0], p[1], p[2], p[3]))
                .collect();
            let img = imgref::Img::new(pixels, frame.image.width() as usize, frame.image.height() as usize);
            let pts = frame.timestamp_ms as f64 / 1000.0;
            collector
                .add_frame_rgba(i, img, pts)
                .map_err(|e| EncodeError::Encoding(e.to_string()))?;
        }
        Ok(())
    });

    let file = File::create(output_path).map_err(EncodeError::Io)?;
    let mut reporter = CallbackProgress { total: frames.len(), current: 0, callback: progress };
    writer
        .write(file, &mut reporter)
        .map_err(|e| EncodeError::Encoding(e.to_string()))?;

    collect_handle.join().expect("collector thread panicked")?;
    Ok(())
}
