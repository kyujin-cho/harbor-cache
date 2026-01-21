# Harbor Cache
A Rust based, lightweight Harbor proxy server to cache downloaded artifacts from upstream

## Key features
- GUI-based cache & configuration management
- Role based account management (read only or read & write)
- Bi-directional proxy server - should support both pulling from and pushing to upstream Harbor registry
- Multi-architecture 

## Configurable items
- Cache retention period
- Caching algorithm
- Upstream connection info
  - URL
  - Registry name
  - Connection protocol (HTTP or HTTPS)
  - SSL certificate validity check skip (applicable for HTTPS connection only)
- Cache storage backend
  - Local Disk
  - S3-compatible
