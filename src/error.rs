use thiserror::Error;

#[derive(Debug, Error)]
pub enum SiftxError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Unknown Analyzer: {0}")]
    UnknownTextAnalyzer(String)
}