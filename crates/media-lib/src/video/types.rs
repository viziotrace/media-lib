use ffmpeg_next::frame;

#[derive(Debug, Clone, Copy)]
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

pub struct DecodedVideoFrame {
    pub video: frame::Video,
}
