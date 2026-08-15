use capture::{start_capture, Region};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    let (tx, rx) = channel();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let region = Region { x: 50, y: 82, width: 320, height: 240 };
    let handle = start_capture(region, 5, tx, stop_flag.clone());

    thread::sleep(Duration::from_millis(200));
    stop_flag.store(true, Ordering::Relaxed);

    let frames: Vec<_> = rx.iter().collect();
    println!("captured {} frames", frames.len());
    for (i, frame) in frames.iter().enumerate() {
        frame.image.save(format!("/tmp/capture_frame_{i}.png")).unwrap();
    }
    handle.join().unwrap().unwrap();
}
