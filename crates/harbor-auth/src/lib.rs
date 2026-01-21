//! Harbor Cache Authentication and Authorization
//!
//! This crate provides JWT-based authentication and role-based
//! access control for Harbor Cache.

pub mod error;
pub mod jwt;
pub mod middleware;
pub mod password;

pub use error::AuthError;
pub use jwt::{Claims, JwtManager};
pub use middleware::{auth_middleware, require_admin, require_write, AuthUser};
pub use password::{hash_password, verify_password};
