use std::path::PathBuf;

use media_types::{MediaLibError, MediaLibInit};
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
                let output = e
                    .to_owned()
                    .match_owned(|e| e.to_string(), |e| e.to_string());
                write!(f, "{}", output)
            }
            MediaClientError::UnknownError(s) => write!(f, "Unknown error: {}", s),
        }
    }
}
impl std::error::Error for MediaClientError {}

pub fn load(lib: &PathBuf) -> Result<(), MediaClientError> {
    let library = unsafe { libloading::Library::new(lib) }.expect("Failed to load library");
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_can_load_lib() {
        let lib = test::get_media_client_lib();
        assert!(load(&lib).is_ok());
    }
}
