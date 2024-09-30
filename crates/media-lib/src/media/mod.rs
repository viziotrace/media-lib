use crate::MediaLibError;
use ffmpeg::ffi::{AVCodecHWConfig, AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX};
use ffmpeg::format::{input, Pixel};
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{context::Context, flag::Flags};
use ffmpeg::util::frame::video::Video;
use ffmpeg_next::decoder::Decoder;
use ffmpeg_next::ffi::avcodec_get_hw_config;
use ffmpeg_next::{self as ffmpeg, Codec};
use std::path::Path;

const NEED_MORE_DATA: i32 = 35;

unsafe fn configure_hardware_acceleration(decoder: &mut Decoder) -> Result<(), MediaLibError> {
    use ffmpeg_next::ffi::{av_hwdevice_iterate_types, AVHWDeviceType};

    let mut device_type = AVHWDeviceType::AV_HWDEVICE_TYPE_NONE;
    loop {
        device_type = av_hwdevice_iterate_types(device_type);
        println!("device_type: {:?}", device_type);
        if device_type == AVHWDeviceType::AV_HWDEVICE_TYPE_NONE {
            break;
        }
        println!("Available hardware device type: {:?}", device_type);
    }

    let codec = decoder
        .codec()
        .ok_or_else(|| MediaLibError::FFmpegError("Failed to get codec".into()))?;
    let mut i = 0;
    loop {
        let hw_config = avcodec_get_hw_config(codec.as_ptr(), i);
        if hw_config.is_null() {
            println!("hw_config is null");
            break;
        }
        if (*hw_config).methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32 != 0 {
            // Hardware acceleration is supported
            println!("Hardware acceleration is supported");
            println!("hw_config: {:?}", (*hw_config).methods);
            println!("hw_config: {:?}", (*hw_config).device_type);
            return Ok(());
        }
        i += 1;
    }
    Err(MediaLibError::FFmpegError(
        "No hardware acceleration support found".into(),
    ))
}

pub struct KeyframeIterator {
    ictx: ffmpeg::format::context::Input,
    decoder: ffmpeg::decoder::Video,
    scaler: Context,
    video_stream_index: usize,
    pub target_width: u32,
    pub target_height: u32,
    eof_sent: bool,
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
        let ictx =
            input(input_path).map_err(|e| MediaLibError::FFmpegError(e.to_string().into()))?;
        let input = ictx
            .streams()
            .best(Type::Video)
            .ok_or_else(|| MediaLibError::FFmpegError("No video stream found".into()))?;
        let video_stream_index = input.index();

        let mut context_decoder =
            ffmpeg::codec::context::Context::from_parameters(input.parameters())
                .map_err(|e| MediaLibError::FFmpegError(e.to_string().into()))?;

        let mut decoder = context_decoder
            .decoder()
            .video()
            .map_err(|e| MediaLibError::FFmpegError(e.to_string().into()))?;

        unsafe {
            configure_hardware_acceleration(&mut decoder)?;
        }

        let target_height = 360;
        let aspect_ratio = decoder.width() as f32 / decoder.height() as f32;
        let target_width = (target_height as f32 * aspect_ratio).round() as u32;

        let scaler = Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            Pixel::RGBA,
            target_width,
            target_height,
            Flags::BILINEAR,
        )
        .map_err(|e| MediaLibError::FFmpegError(e.to_string().into()))?;

        Ok(KeyframeIterator {
            ictx,
            decoder,
            scaler,
            video_stream_index,
            target_width,
            target_height,
            eof_sent: false,
        })
    }

    pub fn get(&mut self) -> Option<Item> {
        while !self.eof_sent {
            match self.ictx.packets().next() {
                Some((stream, packet)) => {
                    if stream.index() == self.video_stream_index {
                        if let Err(e) = self.decoder.send_packet(&packet) {
                            return Some(Err(MediaLibError::FFmpegError(e.to_string().into())));
                        }
                        break; // Exit the loop after sending a video packet
                    }
                    // Continue looping if it's not a video packet
                }
                None => {
                    if let Err(e) = self.decoder.send_eof() {
                        return Some(Err(MediaLibError::FFmpegError(e.to_string().into())));
                    }
                    self.eof_sent = true;
                    break; // Exit the loop after sending EOF
                }
            }
        }

        let mut decoded = Video::empty();
        match self.decoder.receive_frame(&mut decoded) {
            Ok(_) => {
                if decoded.is_key() {
                    let mut rgb_frame = Video::empty();
                    if let Err(e) = self.scaler.run(&decoded, &mut rgb_frame) {
                        return Some(Err(MediaLibError::FFmpegError(e.to_string().into())));
                    }

                    let width = rgb_frame.width();
                    let height = rgb_frame.height();
                    let buffer = rgb_frame.data(0);
                    let jpeg_buffer = get_jpeg_buffer(buffer, width, height);

                    match jpeg_buffer {
                        Ok(img) => Some(Ok(img)),
                        Err(e) => Some(Err(e)),
                    }
                } else {
                    self.get()
                }
            }
            Err(ffmpeg::Error::Other { errno }) => {
                if errno == NEED_MORE_DATA {
                    self.get()
                } else {
                    Some(Err(MediaLibError::FFmpegError(errno.to_string().into())))
                }
            }
            Err(ffmpeg::Error::Eof) => None,
            Err(e) => Some(Err(MediaLibError::FFmpegError(e.to_string().into()))),
        }
    }
}
