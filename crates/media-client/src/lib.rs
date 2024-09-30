use std::path::PathBuf;

use media_types::{MediaKeyFrameIterator, MediaLibError, MediaLibInit};
use stabby::libloading::StabbyLibrary;

#[cfg(test)]
mod test;
pub use media_types;

#[derive(Debug)]
pub enum MediaClientError {
    MediaLibError(MediaLibError),
    UnknownError(String),
}

impl From<MediaLibError> for MediaClientError {
    fn from(error: MediaLibError) -> Self {
        MediaClientError::MediaLibError(error)
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for MediaClientError {
    fn from(error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        MediaClientError::UnknownError(error.to_string())
    }
}

impl std::fmt::Display for MediaClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaClientError::MediaLibError(e) => {
                let output = e.to_owned().match_owned(
                    |e| e.to_string(),
                    |e| e.to_string(),
                    |e| e.to_string(),
                );
                write!(f, "{}", output)
            }
            MediaClientError::UnknownError(s) => write!(f, "Unknown error: {}", s),
        }
    }
}
impl std::error::Error for MediaClientError {}

pub struct MediaClient {
    pub(crate) get_key_frames_ffi: extern "C" fn(
        stabby::string::String,
    ) -> stabby::result::Result<
        stabby::dynptr!(stabby::boxed::Box<dyn MediaKeyFrameIterator>),
        MediaLibError,
    >,
}

impl MediaClient {
    pub fn get_key_frames(
        &self,
        input: &str,
    ) -> Result<stabby::dynptr!(stabby::boxed::Box<dyn MediaKeyFrameIterator>), MediaClientError>
    {
        let input_str = stabby::string::String::from(input);
        let key_frame_interface = (self.get_key_frames_ffi)(input_str);
        let out = key_frame_interface.match_owned(
            |key_frame_iter| std::result::Result::Ok(key_frame_iter),
            |e| std::result::Result::Err(MediaClientError::MediaLibError(e)),
        );
        out
    }
}

pub fn load(lib: &PathBuf) -> Result<MediaClient, MediaClientError> {
    let library = unsafe { libloading::Library::new(lib) }
        .map_err(|e| MediaClientError::UnknownError(e.to_string()))?;

    let init_media_lib = unsafe {
        library
            .get_stabbied::<extern "C" fn() -> stabby::result::Result<MediaLibInit, MediaLibError>>(
                b"init_media_lib",
            )
    }?;

    init_media_lib().match_owned(
        |init| std::result::Result::Ok(init),
        |e| std::result::Result::Err(MediaClientError::MediaLibError(e)),
    )?;

    let get_key_frames = unsafe {
        library.get_stabbied::<extern "C" fn(
            stabby::string::String,
        ) -> stabby::result::Result<
            stabby::dynptr!(stabby::boxed::Box<dyn MediaKeyFrameIterator>),
            MediaLibError,
        >>(b"get_key_frames")
    }?
    .to_owned();

    Ok(MediaClient {
        get_key_frames_ffi: get_key_frames,
    })
}

#[cfg(test)]
mod tests {
    use media_types::MediaKeyFrameIteratorDynMut;

    use super::*;

    #[test]
    fn it_can_get_key_frames() {
        let lib = test::get_media_client_lib();
        let client = load(&lib).unwrap();

        // Load the test movie file
        let test_movie = test::get_test_data_file("test.mp4");

        // Call get_key_frames with the test movie path
        let key_frames_result = client.get_key_frames(test_movie.to_str().unwrap());

        // Assert that the result is Ok
        assert!(key_frames_result.is_ok(), "Failed to get key frames");

        // Unwrap the result to get the MediaKeyFrameIterator
        let mut key_frame_iterator = key_frames_result.unwrap();

        // Get the first key frame to ensure it works
        let first_frame = key_frame_iterator.get_keyframe();
        assert!(
            first_frame.is_some(),
            "No key frames found in the test video"
        );

        assert!(
            first_frame.unwrap().unwrap().len() > 0,
            "No key frames found in the test video"
        );

        // Check the dimensions
        let width = key_frame_iterator.get_width();
        let height = key_frame_iterator.get_height();
        assert!(width > 0 && height > 0, "Invalid frame dimensions");

        loop {
            let frame = key_frame_iterator.get_keyframe();
            if frame.is_none() {
                break;
            }
            frame.unwrap().unwrap();
        }
    }
}
