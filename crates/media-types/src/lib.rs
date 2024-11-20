use stabby::boxed::Box;
use stabby::dynptr;
use stabby::option::Option;
use stabby::result::Result;
use stabby::slice::Slice;
use stabby::string::String;

#[stabby::stabby]
#[repr(u8)]
#[derive(Debug, Clone)]
pub enum VideoSize {
    P240,  // 426x240
    P360,  // 640x360
    P480,  // 854x480
    P720,  // 1280x720
    P1080, // 1920x1080
}

impl VideoSize {
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            VideoSize::P240 => (426, 240),
            VideoSize::P360 => (640, 360),
            VideoSize::P480 => (854, 480),
            VideoSize::P720 => (1280, 720),
            VideoSize::P1080 => (1920, 1080),
        }
    }
}

#[stabby::stabby]
#[repr(stabby)]
#[derive(Debug, Clone)]
pub enum MediaLibError {
    FFmpegError(String),
    UnknownError(String),
    ImageError(String),
}

impl std::fmt::Display for MediaLibError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = self.match_ref(|e| e.to_string(), |e| e.to_string(), |e| e.to_string());
        write!(f, "{}", output)
    }
}

#[stabby::stabby]
pub trait VideoFrame {
    extern "C" fn get_width(&self) -> u32;
    extern "C" fn get_height(&self) -> u32;
    extern "C" fn get_pts(&self) -> i64;
    extern "C" fn get_best_effort_timestamp(&self) -> i64;
    extern "C" fn get_pkt_dts(&self) -> i64;
    extern "C" fn get_pkt_duration(&self) -> i64;
    extern "C" fn get_pkt_pos(&self) -> i64;
    extern "C" fn get_key_frame(&self) -> i32;
    extern "C" fn get_pict_type(&self) -> i32;
    extern "C" fn get_quality(&self) -> i32;
    extern "C" fn get_repeat_pict(&self) -> i32;
    extern "C" fn get_interlaced_frame(&self) -> i32;
    extern "C" fn get_top_field_first(&self) -> i32;
    extern "C" fn get_palette_has_changed(&self) -> i32;
    extern "C" fn get_sample_rate(&self) -> i32;
    extern "C" fn get_format(&self) -> i32;
    extern "C" fn data(&self, idx: usize) -> Slice<u8>;
    extern "C" fn stride(&self, idx: usize) -> usize;
}

pub type VideoFrameResult = Result<dynptr!(Box<dyn VideoFrame>), MediaLibError>;
pub type VideoFrameTrait = dynptr!(Box<dyn VideoFrame>);

#[stabby::stabby]
#[derive(Debug, Clone)]
pub struct MediaFrameDecoderOptions {
    pub target_width: u32,
    pub target_height: u32,
}

#[stabby::stabby]
pub trait MediaFrameDecoder {
    extern "C" fn get_frame(&mut self) -> Option<VideoFrameResult>;
}

#[stabby::stabby]
pub struct MediaLibInit {}
