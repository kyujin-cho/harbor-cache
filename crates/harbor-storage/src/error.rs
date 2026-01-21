//! Storage error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Object not found: {0}")]
    NotFound(String),

    #[error("Invalid digest: {0}")]
    InvalidDigest(String),

    #[error("Digest mismatch: expected {expected}, got {actual}")]
    DigestMismatch { expected: String, actual: String },

    #[error("Storage backend error: {0}")]
    Backend(String),

    #[error("S3 error: {0}")]
    S3(String),

    #[error("Configuration error: {0}")]
    Configuration(String),
}
