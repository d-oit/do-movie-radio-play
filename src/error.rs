use thiserror::Error;

#[derive(Debug, Error)]
pub enum TimelineError {
    #[error("input file does not exist: {0}")]
    MissingInput(String),
    #[error("no decodable audio stream found")]
    EmptyAudio,
    #[error("decode failure: {0}")]
    Decode(String),
    #[error("invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("invalid subtitle: {0}")]
    #[allow(dead_code)]
    InvalidSubtitle(String),
}

impl From<std::io::Error> for TimelineError {
    fn from(err: std::io::Error) -> Self {
        TimelineError::InvalidConfig(err.to_string())
    }
}
