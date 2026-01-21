//! Harbor Cache Storage Layer
//!
//! This crate provides storage abstraction for Harbor Cache,
//! supporting local disk and S3-compatible backends.

pub mod backend;
pub mod error;
pub mod local;

pub use backend::StorageBackend;
pub use error::StorageError;
pub use local::LocalStorage;
