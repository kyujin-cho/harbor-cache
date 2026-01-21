//! Proxy error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Upstream not found: {0}")]
    NotFound(String),

    #[error("Upstream unauthorized")]
    Unauthorized,

    #[error("Upstream returned error: {status} - {message}")]
    UpstreamError { status: u16, message: String },

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Token refresh failed")]
    TokenRefreshFailed,
}
