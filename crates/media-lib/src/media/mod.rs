mod hardware_accelerated_video_decoder;
use crate::MediaLibError;
use ffmpeg::software::scaling::{context::Context, flag::Flags};
use ffmpeg::util::frame::video::Video;
use ffmpeg_next::{self as ffmpeg};
use hardware_accelerated_video_decoder::HardwareAcceleratedVideoDecoder;
use std::path::Path;

pub struct KeyframeIterator {
    scaler: Option<Context>,
    video_decoder: HardwareAcceleratedVideoDecoder,
    pub target_width: u32,
    pub target_height: u32,
}

type Item = Result<Vec<u8>, MediaLibError>;

fn get_jpeg_buffer(slice: &[u8], width: u32, height: u32) -> Result<Vec<u8>, MediaLibError> {
    std::panic::catch_unwind(|| {
        let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_EXT_RGBA);

        comp.set_size(width as usize, height as usize);
        let mut comp = comp
            .start_compress(Vec::new())
            .map_err(|e| MediaLibError::ImageError(e.to_string().into()))?; // any io::Write will work

        // replace with your image data
        comp.write_scanlines(&slice)
            .map_err(|e| MediaLibError::ImageError(e.to_string().into()))?;

        let writer = comp
            .finish()
            .map_err(|e| MediaLibError::ImageError(e.to_string().into()))?;
        Ok(writer)
    })
    .map_err(|e| MediaLibError::UnknownError(format!("Panic in get_jpeg_buffer: {:?}", e).into()))?
}

impl KeyframeIterator {
    pub fn new(input_path: &Path) -> Result<Self, MediaLibError> {
        let video_decoder = unsafe { HardwareAcceleratedVideoDecoder::new(input_path) }?;

        // TODO: make this configurable
        let target_height = 360;
        let aspect_ratio = video_decoder.width() as f32 / video_decoder.height() as f32;
        let target_width = (target_height as f32 * aspect_ratio).round() as u32;

        Ok(KeyframeIterator {
            video_decoder,
            scaler: None,
            target_width,
            target_height,
        })
    }

    fn run_scaler(&mut self, reference_frame: &Video) -> Result<Video, MediaLibError> {
        // This is a little clunky but we don't really know up front what the
        // format will be so we need to check and conditionally create the
        // scaler once we know what the format is.
        let scaler_ref = match &mut self.scaler {
            Some(scaler) => scaler,
            None => {
                let scaler = Context::get(
                    reference_frame.format(),
                    reference_frame.width(),
                    reference_frame.height(),
                    ffmpeg_next::format::Pixel::RGBA,
                    self.target_width,
                    self.target_height,
                    Flags::BILINEAR,
                )
                .map_err(|e| MediaLibError::FFmpegError(e.to_string().into()))?;
                self.scaler = Some(scaler);
                self.scaler
                    .as_mut()
                    .ok_or(MediaLibError::FFmpegError("Failed to create scaler".into()))?
            }
        };

        let mut rgb_frame = Video::empty();
        if let Err(e) = scaler_ref.run(&reference_frame, &mut rgb_frame) {
            return Err(MediaLibError::FFmpegError(e.to_string().into()));
        }

        Ok(rgb_frame)
    }

    pub fn get(&mut self) -> Option<Item> {
        let result = self.video_decoder.get_frame()?;
        match result {
            Ok(decoded) => match self.run_scaler(&decoded) {
                Ok(rgb_frame) => {
                    let width = rgb_frame.width();
                    let height = rgb_frame.height();
                    let buffer = rgb_frame.data(0);
                    let jpeg_buffer = get_jpeg_buffer(buffer, width, height);
                    Some(jpeg_buffer)
                }
                Err(e) => Some(Err(MediaLibError::FFmpegError(e.to_string().into()))),
            },
            Err(e) => Some(Err(MediaLibError::FFmpegError(e.to_string().into()))),
        }
    }
}
