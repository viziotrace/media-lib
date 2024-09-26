use ffmpeg_next as ffmpeg;
use media_types::{MediaLibError, MediaLibInit};

#[stabby::stabby]
#[stabby::export]
pub fn init_media_lib() -> stabby::result::Result<MediaLibInit, MediaLibError> {
    let init = ffmpeg::init();
    match init {
        Ok(_) => Ok(MediaLibInit {}).into(),
        Err(e) => Err(MediaLibError::FFmpegError(e.to_string().into())).into(),
    }
}
