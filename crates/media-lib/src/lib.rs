pub mod video;
use std::path::Path;

use ffmpeg_next as ffmpeg;
use media_types::{
    MediaFrameDecoder, MediaFrameDecoderOptions, MediaLibError, MediaLibInit, VideoFrame,
    VideoFrameResult,
};
use video::{DecodedVideoFrame, HardwareAcceleratedVideoDecoder};

#[stabby::stabby]
#[stabby::export]
pub fn init_media_lib() -> stabby::result::Result<MediaLibInit, MediaLibError> {
    let init = ffmpeg::init();
    match init {
        Ok(_) => Ok(MediaLibInit {}).into(),
        Err(e) => Err(MediaLibError::FFmpegError(e.to_string().into())).into(),
    }
}

struct MediaFrameDecoderWrapper {
    decoder: HardwareAcceleratedVideoDecoder,
}

struct VideoFrameWrapper {
    inner: DecodedVideoFrame,
}

impl VideoFrame for VideoFrameWrapper {
    extern "C" fn get_width(&self) -> u32 {
        self.inner.video.width()
    }

    extern "C" fn get_height(&self) -> u32 {
        self.inner.video.height()
    }

    extern "C" fn get_pts(&self) -> i64 {
        self.inner.video.pts().unwrap_or(0)
    }

    extern "C" fn get_pkt_dts(&self) -> i64 {
        self.inner.video.packet().dts
    }

    extern "C" fn get_pkt_duration(&self) -> i64 {
        self.inner.video.packet().duration
    }

    extern "C" fn get_pkt_pos(&self) -> i64 {
        self.inner.video.packet().position
    }

    extern "C" fn get_key_frame(&self) -> i32 {
        self.inner.video.is_key() as i32
    }

    extern "C" fn get_quality(&self) -> i32 {
        unsafe { (*self.inner.video.as_ptr()).quality }
    }

    extern "C" fn get_interlaced_frame(&self) -> i32 {
        unsafe { (*self.inner.video.as_ptr()).interlaced_frame }
    }

    extern "C" fn get_top_field_first(&self) -> i32 {
        unsafe { (*self.inner.video.as_ptr()).top_field_first }
    }

    extern "C" fn get_palette_has_changed(&self) -> i32 {
        unsafe { (*self.inner.video.as_ptr()).palette_has_changed }
    }

    extern "C" fn get_sample_rate(&self) -> i32 {
        unsafe { (*self.inner.video.as_ptr()).sample_rate }
    }

    extern "C" fn get_format(&self) -> i32 {
        unsafe { (*self.inner.video.as_ptr()).format }
    }

    extern "C" fn get_best_effort_timestamp(&self) -> i64 {
        unsafe { (*self.inner.video.as_ptr()).best_effort_timestamp }
    }

    extern "C" fn get_pict_type(&self) -> i32 {
        unsafe { (*self.inner.video.as_ptr()).pict_type as i32 }
    }

    extern "C" fn get_repeat_pict(&self) -> i32 {
        unsafe { (*self.inner.video.as_ptr()).repeat_pict }
    }

    extern "C" fn data_ptr(&self) -> *const u8 {
        self.inner.video.data(0).as_ptr()
    }

    extern "C" fn data_len(&self) -> usize {
        self.inner.video.data(0).len()
    }
}

impl MediaFrameDecoder for MediaFrameDecoderWrapper {
    extern "C" fn get_frame(&mut self) -> stabby::option::Option<VideoFrameResult> {
        match self.decoder.get_frame() {
            Some(Ok(frame)) => {
                let wrapper = VideoFrameWrapper { inner: frame };
                Some(Ok(stabby::boxed::Box::new(wrapper).into()).into()).into()
            }
            Some(Err(e)) => Some(Err(e).into()).into(),
            None => None.into(),
        }
    }
}

#[stabby::stabby]
#[stabby::export]
pub fn new_frame_decoder(
    path_str: stabby::string::String,
    options: MediaFrameDecoderOptions,
) -> stabby::result::Result<stabby::dynptr!(stabby::boxed::Box<dyn MediaFrameDecoder>), MediaLibError>
{
    let path_str = path_str.to_string();
    let path = Path::new(&path_str);
    let decoder =
        HardwareAcceleratedVideoDecoder::new(path, options.target_width, options.target_height);

    match decoder {
        Ok(decoder) => {
            let wrapper = MediaFrameDecoderWrapper { decoder };
            Ok(stabby::boxed::Box::new(wrapper).into()).into()
        }
        Err(e) => Err(e).into(),
    }
}

#[stabby::stabby]
#[stabby::export]
pub fn init_logging() {
    let log_level = ffmpeg::util::log::Level::Info;
    ffmpeg::util::log::set_level(log_level);
    let _ = env_logger::try_init();
}
