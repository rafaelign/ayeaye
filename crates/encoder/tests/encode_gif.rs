use editor::Frame;
use encoder::{encode_gif, EncodeError};
use image::RgbaImage;
use std::sync::{Arc, Mutex};

#[test]
fn encode_gif_writes_a_valid_multi_frame_gif() {
    let frames = vec![
        Frame { image: RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255])), timestamp_ms: 0 },
        Frame { image: RgbaImage::from_pixel(4, 4, image::Rgba([0, 255, 0, 255])), timestamp_ms: 200 },
        Frame { image: RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 255, 255])), timestamp_ms: 400 },
    ];
    let output = std::env::temp_dir().join("encoder_test_output.gif");

    let progress_calls = Arc::new(Mutex::new(0usize));
    let progress_calls_clone = progress_calls.clone();

    encode_gif(&frames, &output, move |current, total| {
        assert!(current <= total);
        *progress_calls_clone.lock().unwrap() += 1;
    })
    .expect("encode_gif should succeed");

    assert!(*progress_calls.lock().unwrap() > 0);

    let file = std::fs::File::open(&output).unwrap();
    let decoder = gif::DecodeOptions::new();
    let mut reader = decoder.read_info(file).unwrap();
    let mut frame_count = 0;
    while reader.read_next_frame().unwrap().is_some() {
        frame_count += 1;
    }
    assert_eq!(frame_count, 3);

    std::fs::remove_file(&output).ok();
}

#[test]
fn encode_gif_rejects_empty_frame_list() {
    let output = std::env::temp_dir().join("encoder_test_empty.gif");
    let err = encode_gif(&[], &output, |_, _| {}).unwrap_err();
    assert!(matches!(err, EncodeError::NoFrames));
}
