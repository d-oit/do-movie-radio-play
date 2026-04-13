use thiserror::Error;

#[derive(Debug, Error)]
pub enum TimelineError {
    #[error("input file does not exist: {0}")]
    MissingInput(String),
    #[error("no decodable audio stream found")]
    EmptyAudio,
    #[error("decode failure: {0}")]
    Decode(String),
}
