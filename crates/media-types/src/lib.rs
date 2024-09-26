use stabby::string::String;

#[stabby::stabby]
#[repr(stabby)]
#[derive(Debug, Clone)]
pub enum MediaLibError {
    FFmpegError(String),
    UnknownError(String),
}

impl std::fmt::Display for MediaLibError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = self.match_ref(|e| e.to_string(), |e| e.to_string());
        write!(f, "{}", output)
    }
}

#[stabby::stabby]
pub struct MediaLibInit {}
