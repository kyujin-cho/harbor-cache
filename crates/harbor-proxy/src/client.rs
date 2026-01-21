//! Harbor upstream client

use bytes::Bytes;
use reqwest::{Client, Response, StatusCode};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::error::ProxyError;

/// Harbor client configuration
#[derive(Clone, Debug)]
pub struct HarborClientConfig {
    /// Base URL of the upstream Harbor registry
    pub url: String,
    /// Registry/project name
    pub registry: String,
    /// Username for authentication
    pub username: Option<String>,
    /// Password for authentication
    pub password: Option<String>,
    /// Skip TLS certificate verification
    pub skip_tls_verify: bool,
}

/// Token response from Harbor
#[derive(Debug, Deserialize)]
struct TokenResponse {
    token: String,
    #[serde(default)]
    #[allow(dead_code)]
    expires_in: Option<u64>,
}

/// Harbor API client
pub struct HarborClient {
    config: HarborClientConfig,
    client: Client,
}

impl HarborClient {
    /// Create a new Harbor client
    pub fn new(config: HarborClientConfig) -> Result<Self, ProxyError> {
        let mut builder = Client::builder();

        if config.skip_tls_verify {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder.build()?;

        info!("Created Harbor client for {}", config.url);

        Ok(Self { config, client })
    }

    /// Get the registry URL for OCI operations
    #[allow(dead_code)]
    fn registry_url(&self) -> String {
        format!("{}/v2/{}", self.config.url, self.config.registry)
    }

    /// Parse WWW-Authenticate header and fetch token with proper scope
    async fn fetch_token_for_scope(&self, www_auth: &str) -> Result<String, ProxyError> {
        // Parse: Bearer realm="https://...",service="harbor-registry",scope="..."
        if !www_auth.starts_with("Bearer ") {
            return Err(ProxyError::InvalidResponse(
                "Expected Bearer authentication".to_string(),
            ));
        }

        let parts: Vec<&str> = www_auth[7..].split(',').collect();
        let mut realm = None;
        let mut service = None;
        let mut scope = None;

        for part in parts {
            let kv: Vec<&str> = part.splitn(2, '=').collect();
            if kv.len() == 2 {
                let key = kv[0].trim();
                let value = kv[1].trim().trim_matches('"');
                match key {
                    "realm" => realm = Some(value),
                    "service" => service = Some(value),
                    "scope" => scope = Some(value),
                    _ => {}
                }
            }
        }

        let realm = realm.ok_or(ProxyError::InvalidResponse(
            "Missing realm in WWW-Authenticate".to_string(),
        ))?;

        // Build token request URL
        let mut url = realm.to_string();
        let mut params = vec![];

        if let Some(svc) = service {
            params.push(format!("service={}", svc));
        }
        if let Some(scp) = scope {
            params.push(format!("scope={}", scp));
        }

        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }

        debug!("Fetching token from: {}", url);

        let mut request = self.client.get(&url);

        // Add basic auth if credentials are provided
        if let (Some(username), Some(password)) = (&self.config.username, &self.config.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message: format!("Token request failed: {}", body),
            });
        }

        let token_response: TokenResponse = response.json().await?;

        Ok(format!("Bearer {}", token_response.token))
    }

    /// Make an authenticated request, handling 401 by getting a properly scoped token
    async fn authenticated_request(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(&str, &str)>,
        body: Option<Bytes>,
    ) -> Result<Response, ProxyError> {
        // First attempt without token
        let mut request = match method {
            "GET" => self.client.get(url),
            "HEAD" => self.client.head(url),
            "PUT" => self.client.put(url),
            "POST" => self.client.post(url),
            _ => self.client.get(url),
        };

        for (key, value) in &headers {
            request = request.header(*key, *value);
        }

        if let Some(ref data) = body {
            request = request.body(data.clone());
        }

        let response = request.send().await?;

        // If unauthorized, get a token with the proper scope and retry
        if response.status() == StatusCode::UNAUTHORIZED {
            let www_auth = response
                .headers()
                .get("www-authenticate")
                .and_then(|h| h.to_str().ok())
                .ok_or(ProxyError::Unauthorized)?;

            debug!("Got 401, fetching token with scope from: {}", www_auth);

            let token = self.fetch_token_for_scope(www_auth).await?;

            // Retry with token
            let mut request = match method {
                "GET" => self.client.get(url),
                "HEAD" => self.client.head(url),
                "PUT" => self.client.put(url),
                "POST" => self.client.post(url),
                _ => self.client.get(url),
            };

            request = request.header("Authorization", &token);

            for (key, value) in &headers {
                request = request.header(*key, *value);
            }

            if let Some(data) = body {
                request = request.body(data);
            }

            return Ok(request.send().await?);
        }

        Ok(response)
    }

    /// Check if upstream is reachable
    pub async fn ping(&self) -> Result<bool, ProxyError> {
        let url = format!("{}/v2/", self.config.url);
        let response = self.authenticated_request("GET", &url, vec![], None).await?;
        Ok(response.status().is_success())
    }

    /// Get a manifest from upstream
    pub async fn get_manifest(
        &self,
        repository: &str,
        reference: &str,
    ) -> Result<(Bytes, String, String), ProxyError> {
        let url = format!(
            "{}/v2/{}/{}/manifests/{}",
            self.config.url, self.config.registry, repository, reference
        );

        debug!("Fetching manifest: {}", url);

        let headers = vec![(
            "Accept",
            "application/vnd.oci.image.manifest.v1+json, \
             application/vnd.oci.image.index.v1+json, \
             application/vnd.docker.distribution.manifest.v2+json, \
             application/vnd.docker.distribution.manifest.list.v2+json, \
             application/vnd.docker.distribution.manifest.v1+prettyjws",
        )];

        let response = self
            .authenticated_request("GET", &url, headers, None)
            .await?;
        let status = response.status();

        if status == StatusCode::NOT_FOUND {
            return Err(ProxyError::NotFound(format!(
                "{}:{}",
                repository, reference
            )));
        }

        if !status.is_success() {
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        // Get content type and digest from headers
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("application/vnd.oci.image.manifest.v1+json")
            .to_string();

        let digest = response
            .headers()
            .get("docker-content-digest")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = response.bytes().await?;

        Ok((body, content_type, digest))
    }

    /// Get a blob from upstream
    pub async fn get_blob(
        &self,
        repository: &str,
        digest: &str,
    ) -> Result<(Bytes, u64), ProxyError> {
        let url = format!(
            "{}/v2/{}/{}/blobs/{}",
            self.config.url, self.config.registry, repository, digest
        );

        debug!("Fetching blob: {}", url);

        let response = self
            .authenticated_request("GET", &url, vec![], None)
            .await?;
        let status = response.status();

        if status == StatusCode::NOT_FOUND {
            return Err(ProxyError::NotFound(digest.to_string()));
        }

        if !status.is_success() {
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        let size = response
            .headers()
            .get("content-length")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let body = response.bytes().await?;

        Ok((body, size))
    }

    /// Check if a blob exists
    pub async fn blob_exists(&self, repository: &str, digest: &str) -> Result<bool, ProxyError> {
        let url = format!(
            "{}/v2/{}/{}/blobs/{}",
            self.config.url, self.config.registry, repository, digest
        );

        debug!("Checking blob existence: {}", url);

        let response = self
            .authenticated_request("HEAD", &url, vec![], None)
            .await?;

        Ok(response.status().is_success())
    }

    /// Push a blob to upstream
    pub async fn push_blob(
        &self,
        repository: &str,
        digest: &str,
        data: Bytes,
    ) -> Result<(), ProxyError> {
        // Start upload
        let url = format!(
            "{}/v2/{}/{}/blobs/uploads/",
            self.config.url, self.config.registry, repository
        );

        debug!("Starting blob upload to: {}", url);

        let response = self
            .authenticated_request("POST", &url, vec![], None)
            .await?;

        if !response.status().is_success() && response.status() != StatusCode::ACCEPTED {
            return Err(ProxyError::UpstreamError {
                status: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        // Get upload location
        let location = response
            .headers()
            .get("location")
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| ProxyError::InvalidResponse("Missing Location header".to_string()))?;

        // Complete upload
        let upload_url = if location.starts_with("http") {
            format!("{}?digest={}", location, digest)
        } else {
            format!("{}{}?digest={}", self.config.url, location, digest)
        };

        debug!("Completing blob upload: {}", upload_url);

        let headers = vec![("Content-Type", "application/octet-stream")];

        let response = self
            .authenticated_request("PUT", &upload_url, headers, Some(data))
            .await?;

        if !response.status().is_success() && response.status() != StatusCode::CREATED {
            return Err(ProxyError::UpstreamError {
                status: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        Ok(())
    }

    /// Push a manifest to upstream
    pub async fn push_manifest(
        &self,
        repository: &str,
        reference: &str,
        data: Bytes,
        content_type: &str,
    ) -> Result<String, ProxyError> {
        let url = format!(
            "{}/v2/{}/{}/manifests/{}",
            self.config.url, self.config.registry, repository, reference
        );

        debug!("Pushing manifest to: {}", url);

        let headers = vec![("Content-Type", content_type)];

        let response = self
            .authenticated_request("PUT", &url, headers, Some(data))
            .await?;
        let status = response.status();

        if !status.is_success() && status != StatusCode::CREATED {
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        // Get digest from response
        let digest = response
            .headers()
            .get("docker-content-digest")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();

        Ok(digest)
    }
}

/// Simple base64 encoding
fn base64_encode(input: &str) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut encoder =
            base64::write::EncoderWriter::new(&mut buf, &base64::engine::general_purpose::STANDARD);
        encoder.write_all(input.as_bytes()).unwrap();
    }
    String::from_utf8(buf).unwrap()
}
