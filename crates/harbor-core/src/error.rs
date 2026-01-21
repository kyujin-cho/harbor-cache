//! Core error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Database error: {0}")]
    Database(#[from] harbor_db::DbError),

    #[error("Storage error: {0}")]
    Storage(#[from] harbor_storage::StorageError),

    #[error("Proxy error: {0}")]
    Proxy(#[from] harbor_proxy::ProxyError),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid digest: {0}")]
    InvalidDigest(String),

    #[error("Cache miss")]
    CacheMiss,
}
