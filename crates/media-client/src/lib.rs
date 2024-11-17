use std::path::PathBuf;

use libloading::Library;
use media_types::{MediaFrameDecoder, MediaFrameDecoderOptions, MediaLibError, MediaLibInit};
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
    library: Library,
}

impl MediaClient {
    pub fn new_frame_decoder(
        &self,
        input: &str,
        options: MediaFrameDecoderOptions,
    ) -> Result<stabby::dynptr!(stabby::boxed::Box<dyn MediaFrameDecoder>), MediaClientError> {
        let new_frame_decoder = unsafe {
            self.library.get_stabbied::<extern "C" fn(
                stabby::string::String,
                MediaFrameDecoderOptions,
            ) -> stabby::result::Result<
                stabby::dynptr!(stabby::boxed::Box<dyn MediaFrameDecoder>),
                MediaLibError,
            >>(b"new_frame_decoder")
        }
        .map_err(|e| MediaClientError::UnknownError(e.to_string()))?;

        let input_str = stabby::string::String::from(input);
        let decoder = (new_frame_decoder)(input_str, options);
        decoder.match_owned(
            |decoder| std::result::Result::Ok(decoder),
            |e| std::result::Result::Err(MediaClientError::MediaLibError(e)),
        )
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

    let init_logging = unsafe { library.get_stabbied::<extern "C" fn()>(b"init_logging") }?;
    init_logging();

    Ok(MediaClient { library: library })
}

#[cfg(test)]
mod tests {
    use super::*;
    use media_types::{MediaFrameDecoderDynMut, VideoFrameDyn};

    #[test]
    fn it_can_decode_frames() {
        let lib = test::get_media_client_lib();
        let client = load(&lib).unwrap();

        // Load the test movie file
        let test_movie = test::get_test_data_file("test.mp4");
        let (target_width, target_height) = media_types::VideoSize::P240.dimensions();

        // Create frame decoder
        let mut decoder = client
            .new_frame_decoder(
                test_movie.to_str().unwrap(),
                MediaFrameDecoderOptions {
                    target_width,
                    target_height,
                },
            )
            .unwrap();

        // Get first frame to verify it works
        let first_frame = decoder.get_frame();
        assert!(first_frame.is_some(), "No frames found in test video");

        // check that frame has correct dimensions
        let first_frame = first_frame.unwrap();
        let frame = first_frame.unwrap();
        assert_eq!(frame.get_width(), 426);
        assert_eq!(frame.get_height(), 240);

        let mut frame_count = 1;
        // Read remaining frames
        loop {
            let frame = decoder.get_frame();
            if frame.is_none() {
                break;
            }
            println!("Decoded {} frames", frame_count);
            frame.unwrap().unwrap();
            frame_count += 1;
        }
        assert!(frame_count > 0, "No frames decoded from test video");
    }
}
