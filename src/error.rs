use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum SiftxError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

}