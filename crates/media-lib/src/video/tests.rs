#[cfg(test)]
mod tests {
    use super::super::{HardwareAcceleratedVideoDecoder, VideoSize};
    use crate::video::hardware::HardwareContext;
    use ffmpeg_next::ffi::{AVHWDeviceType, AVPixelFormat};
    use std::path::PathBuf;

    #[test]
    fn test_decode_video() {
        ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Info);
        let out = env_logger::builder().is_test(true).try_init();
        println!("Logger initialized: {:?}", out);
        let test_video_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/test.mp4");
        let (target_width, target_height) = VideoSize::P240.dimensions();

        let mut decoder =
            HardwareAcceleratedVideoDecoder::new(&test_video_path, target_width, target_height)
                .expect("Failed to create decoder");

        let mut frame_count = 0;
        let mut first_frame_dimensions = None;

        while let Some(frame_result) = decoder.get_frame() {
            match frame_result {
                Ok(frame) => {
                    frame_count += 1;

                    // Check first frame dimensions
                    if first_frame_dimensions.is_none() {
                        first_frame_dimensions = Some((frame.video.width(), frame.video.height()));

                        // Verify dimensions match requested size
                        assert_eq!(
                            first_frame_dimensions.unwrap(),
                            VideoSize::P240.dimensions(),
                            "Frame dimensions don't match requested size"
                        );
                    }

                    let data = frame.video.data(0);
                    assert!(data.len() > 1, "Video frame should have multiple planes");
                    let pix_fmt = frame.video.format();
                    assert_eq!(pix_fmt, ffmpeg_next::format::Pixel::YUV420P);
                    // For YUV420p, we should have 3 planes - Y, U, and V
                    assert_eq!(frame.video.planes(), 3, "YUV420p should have 3 planes");

                    // Basic sanity check - values should be within valid range (0-255)
                    assert!(
                        frame.video.data(0).iter().all(|&x| x <= 255),
                        "Y values out of range"
                    );
                    assert!(
                        frame.video.data(1).iter().all(|&x| x <= 255),
                        "U values out of range"
                    );
                    assert!(
                        frame.video.data(2).iter().all(|&x| x <= 255),
                        "V values out of range"
                    );

                    // For YUV420p:
                    // Y plane has full resolution (width * height)
                    // U and V planes are quarter resolution ((width/2) * (height/2))
                    let (width, height) = (frame.video.width(), frame.video.height());
                    let y_size = (width * height) as usize;
                    let uv_size = (width * height / 4) as usize;

                    // Verify plane sizes match expected dimensions
                    assert_eq!(frame.video.data(0).len(), y_size, "Y plane size mismatch");
                    assert_eq!(frame.video.data(1).len(), uv_size, "U plane size mismatch");
                    assert_eq!(frame.video.data(2).len(), uv_size, "V plane size mismatch");
                }
                Err(e) => panic!("Failed to decode frame: {}", e),
            }
        }

        assert!(frame_count > 0, "No frames were decoded");
        println!("Successfully decoded {} frames", frame_count);
    }
}
