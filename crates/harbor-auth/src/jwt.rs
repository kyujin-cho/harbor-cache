//! JWT token management

use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::error::AuthError;

/// JWT claims
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Username
    pub username: String,
    /// User role
    pub role: String,
    /// Expiration time (Unix timestamp)
    pub exp: i64,
    /// Issued at (Unix timestamp)
    pub iat: i64,
}

/// JWT manager for token generation and validation
#[derive(Clone)]
pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    token_expiry_hours: i64,
}

impl JwtManager {
    /// Create a new JWT manager
    pub fn new(secret: &str, token_expiry_hours: i64) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            token_expiry_hours,
        }
    }

    /// Generate a JWT token for a user
    pub fn generate_token(
        &self,
        user_id: i64,
        username: &str,
        role: &str,
    ) -> Result<String, AuthError> {
        let now = Utc::now();
        let exp = now + Duration::hours(self.token_expiry_hours);

        let claims = Claims {
            sub: user_id.to_string(),
            username: username.to_string(),
            role: role.to_string(),
            exp: exp.timestamp(),
            iat: now.timestamp(),
        };

        debug!("Generating token for user: {}", username);

        encode(&Header::default(), &claims, &self.encoding_key).map_err(AuthError::Jwt)
    }

    /// Validate a JWT token and return claims
    pub fn validate_token(&self, token: &str) -> Result<Claims, AuthError> {
        let validation = Validation::default();

        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)?;

        // Check expiration
        let now = Utc::now().timestamp();
        if token_data.claims.exp < now {
            return Err(AuthError::TokenExpired);
        }

        Ok(token_data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_generation_and_validation() {
        let manager = JwtManager::new("test-secret-key", 24);

        let token = manager.generate_token(1, "testuser", "admin").unwrap();
        let claims = manager.validate_token(&token).unwrap();

        assert_eq!(claims.sub, "1");
        assert_eq!(claims.username, "testuser");
        assert_eq!(claims.role, "admin");
    }

    #[test]
    fn test_invalid_token() {
        let manager = JwtManager::new("test-secret-key", 24);

        let result = manager.validate_token("invalid-token");
        assert!(result.is_err());
    }
}
