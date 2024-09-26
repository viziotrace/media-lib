use stabby::option::Option;
use stabby::result::Result;
use stabby::string::String;
use stabby::vec::Vec;

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

pub type MediaKeyFrame = Vec<u8>;
pub type MediaKeyFrameGet = Result<MediaKeyFrame, MediaLibError>;

#[stabby::stabby]
pub trait MediaKeyFrameIterator {
    extern "C" fn get_keyframe(&mut self) -> Option<MediaKeyFrameGet>;
    extern "C" fn get_width(&mut self) -> u32;
    extern "C" fn get_height(&mut self) -> u32;
}

#[stabby::stabby]
pub struct MediaLibInit {}
