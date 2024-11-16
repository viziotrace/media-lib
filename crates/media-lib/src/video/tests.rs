#[cfg(test)]
mod tests {
    use super::super::{HardwareAcceleratedVideoDecoder, VideoSize};
    use crate::video::hardware::HardwareContext;
    use ffmpeg_next::ffi::{AVHWDeviceType, AVPixelFormat};
    use std::path::PathBuf;

    #[test]
    fn test_decode_video() {
        ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Debug);
        let test_video_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/test.mp4");

        let mut decoder = HardwareAcceleratedVideoDecoder::new(&test_video_path, VideoSize::P240)
            .expect("Failed to create decoder");

        let mut frame_count = 0;
        let mut first_frame_dimensions = None;

        while let Some(frame_result) = decoder.get_frame() {
            match frame_result {
                Ok(frame) => {
                    frame_count += 1;

                    // Check first frame dimensions
                    if first_frame_dimensions.is_none() {
                        first_frame_dimensions = Some((frame.frame.width(), frame.frame.height()));

                        // Verify dimensions match requested size
                        assert_eq!(
                            first_frame_dimensions.unwrap(),
                            VideoSize::P720.dimensions(),
                            "Frame dimensions don't match requested size"
                        );
                    }

                    // Verify frame data is valid RGBA
                    let data = frame.frame.data(0);
                    assert!(!data.is_empty(), "Frame data is empty");
                    assert_eq!(
                        data.len(),
                        (frame.frame.width() * frame.frame.height() * 4) as usize,
                        "Incorrect RGBA data size"
                    );
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
