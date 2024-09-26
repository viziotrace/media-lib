use ffmpeg_next as ffmpeg;
use stabby::string::String;

#[stabby::stabby]
#[repr(stabby)]
pub enum MediaLibError {
    FFmpegError(String),
    UnknownError(String),
}

impl From<ffmpeg::Error> for MediaLibError {
    fn from(error: ffmpeg::Error) -> Self {
        MediaLibError::FFmpegError(error.to_string().into())
    }
}

#[stabby::stabby]
struct MediaLibInit {}

#[stabby::stabby]
#[stabby::export]
pub fn init_media_lib() -> stabby::result::Result<MediaLibInit, MediaLibError> {
    let init = ffmpeg::init();
    match init {
        Ok(_) => Ok(MediaLibInit {}).into(),
        Err(e) => Err(MediaLibError::FFmpegError(e.to_string().into())).into(),
    }
}
