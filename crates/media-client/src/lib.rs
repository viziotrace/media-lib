use std::path::PathBuf;

use media_types::{MediaKeyFrameIterator, MediaLibError, MediaLibInit};
use stabby::libloading::StabbyLibrary;

mod test;

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
    pub get_key_frames: extern "C" fn(
        stabby::string::String,
    ) -> stabby::result::Result<
        stabby::dynptr!(stabby::boxed::Box<dyn MediaKeyFrameIterator>),
        MediaLibError,
    >,
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

    let result = init_media_lib();
    if result.is_err() {
        return Err(MediaClientError::MediaLibError(result.err().unwrap()));
    }

    let get_key_frames = unsafe {
        library.get_stabbied::<extern "C" fn(
            stabby::string::String,
        ) -> stabby::result::Result<
            stabby::dynptr!(stabby::boxed::Box<dyn MediaKeyFrameIterator>),
            MediaLibError,
        >>(b"get_key_frames")
    }?
    .to_owned();

    Ok(MediaClient { get_key_frames })
}

#[cfg(test)]
mod tests {
    use media_types::MediaKeyFrameIteratorDynMut;

    use super::*;

    #[test]
    fn it_can_load_lib() {
        let lib = test::get_media_client_lib();
        let client = load(&lib).unwrap();

        // Load the test movie file
        let test_movie = test::get_test_data_file("test.mp4");

        // Convert the PathBuf to a stabby::string::String
        let test_movie_path = stabby::string::String::from(test_movie.to_str().unwrap());

        // Call get_key_frames with the test movie path
        let key_frames_result = (client.get_key_frames)(test_movie_path);

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
            let frame = frame.unwrap().unwrap();
            println!("Frame size: {}", frame.len());
        }
    }
}
