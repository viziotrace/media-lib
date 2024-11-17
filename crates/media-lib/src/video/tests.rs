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

                    // Verify frame data is valid RGBA
                    let data = frame.video.data(0);
                    assert!(data.len() > 1, "Video frame should have multiple planes");
                    let pix_fmt = frame.video.format();
                    assert_eq!(pix_fmt, ffmpeg_next::format::Pixel::RGBA);
                }
                Err(e) => panic!("Failed to decode frame: {}", e),
            }
        }

        assert!(frame_count > 0, "No frames were decoded");
        println!("Successfully decoded {} frames", frame_count);
    }

    #[test]
    fn test_hardware_detection() {
        // Try to create hardware contexts for supported types
        let cuda = HardwareContext::new(
            AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA,
            AVPixelFormat::AV_PIX_FMT_NV12,
        );
        let videotoolbox = HardwareContext::new(
            AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
            AVPixelFormat::AV_PIX_FMT_NV12,
        );

        // At least one should be available on most systems
        assert!(
            cuda.is_ok() || videotoolbox.is_ok(),
            "No hardware acceleration available - tests will use software decoding"
        );

        if let Ok(ctx) = cuda {
            println!("CUDA hardware acceleration available");
        }
        if let Ok(ctx) = videotoolbox {
            println!("VideoToolbox hardware acceleration available");
        }
    }
}
