pub mod media;
use std::path::Path;

use ffmpeg_next as ffmpeg;
use media::KeyframeIterator;
use media_types::{MediaKeyFrameGet, MediaKeyFrameIterator, MediaLibError, MediaLibInit};

#[stabby::stabby]
#[stabby::export]
pub fn init_media_lib() -> stabby::result::Result<MediaLibInit, MediaLibError> {
    let init = ffmpeg::init();
    match init {
        Ok(_) => Ok(MediaLibInit {}).into(),
        Err(e) => Err(MediaLibError::FFmpegError(e.to_string().into())).into(),
    }
}

pub struct MediaKeyFrameIteratorWrapper {
    iterator: KeyframeIterator,
}

impl MediaKeyFrameIterator for MediaKeyFrameIteratorWrapper {
    extern "C" fn get_keyframe(&mut self) -> stabby::option::Option<MediaKeyFrameGet> {
        let frame_option = self.iterator.get();

        match frame_option {
            Some(frame) => match frame {
                Ok(frame) => {
                    // Couldn't figure out how to convert the frame to a stabby vec so we're doing it manually
                    let frame_bytes = frame.to_vec();
                    let mut stabby_vec = stabby::vec::Vec::with_capacity(frame_bytes.len());
                    for (_, byte) in frame_bytes.iter().enumerate() {
                        stabby_vec.push(*byte);
                    }

                    stabby::option::Option::Some(stabby::result::Result::Ok(stabby_vec))
                }
                Err(e) => stabby::option::Option::Some(stabby::result::Result::Err(e)),
            },
            None => stabby::option::Option::None(),
        }
    }

    extern "C" fn get_width(&mut self) -> u32 {
        self.iterator.target_width
    }

    extern "C" fn get_height(&mut self) -> u32 {
        self.iterator.target_height
    }
}

#[stabby::stabby]
#[stabby::export]
pub fn get_key_frames(
    path_str: stabby::string::String,
) -> stabby::result::Result<
    stabby::dynptr!(stabby::boxed::Box<dyn MediaKeyFrameIterator>),
    MediaLibError,
> {
    let path_str = path_str.to_string();
    let path = Path::new(&path_str);
    let iterator = KeyframeIterator::new(path).unwrap();
    let wrapper = MediaKeyFrameIteratorWrapper { iterator };
    Ok(stabby::boxed::Box::new(wrapper).into()).into()
}

#[stabby::stabby]
#[stabby::export]
pub fn init_logging() {
    let log_level = ffmpeg::util::log::Level::Info;
    ffmpeg::util::log::set_level(log_level);
    pretty_env_logger::init();
}
