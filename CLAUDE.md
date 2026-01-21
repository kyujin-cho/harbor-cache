# CLAUDE.md

This file provides guidance to Claude Code when working with this repository.

## Project Overview

**Harbor Cache** is a Rust-based lightweight proxy server designed to cache downloaded artifacts from upstream Harbor registries. It acts as an intermediary that stores container images and artifacts locally, reducing bandwidth usage and improving pull times for frequently accessed images.

## Key Features

- **GUI-based management**: Web interface for cache and configuration management
- **Role-based access control**: Support for read-only and read-write account permissions
- **Bi-directional proxy**: Supports both pulling from and pushing to upstream Harbor registries
- **Multi-architecture support**: Handles images for multiple CPU architectures

## Configuration

The following items are configurable:

- **Cache retention period**: How long cached artifacts are kept
- **Caching algorithm**: Strategy for cache eviction
- **Upstream connection**:
  - URL and registry name
  - Connection protocol (HTTP/HTTPS)
  - SSL certificate validation (skip option for self-signed certs)
- **Storage backend**:
  - Local disk storage
  - S3-compatible object storage

## Build Commands

```bash
# Build in debug mode
cargo build

# Build in release mode
cargo build --release

# Run tests
cargo test

# Run the server with default config
cargo run -- --config config/default.toml

# Run with custom port
cargo run -- --config config/default.toml --port 5001

# Check code without building
cargo check
```

## Project Structure

```
harbor-cache/
├── Cargo.toml                    # Workspace root
├── config/default.toml           # Default configuration
├── data/                         # Runtime data (cache, database)
├── crates/
│   ├── harbor-cache/             # Main binary (CLI, server setup)
│   ├── harbor-core/              # Core business logic (cache manager, registry service)
│   ├── harbor-storage/           # Storage abstraction (local disk, S3)
│   ├── harbor-proxy/             # Upstream Harbor client
│   ├── harbor-api/               # REST API (Axum routes)
│   ├── harbor-auth/              # Authentication/Authorization (JWT, Argon2)
│   └── harbor-db/                # Database layer (SQLite)
└── harbor-setup/                 # Test Harbor environment
```

## API Endpoints

### Health Check
- `GET /health` - Health check endpoint

### OCI Distribution API (v2)
- `GET /v2/` - Version check
- `GET /v2/<name>/manifests/<reference>` - Get manifest
- `HEAD /v2/<name>/manifests/<reference>` - Check manifest exists
- `PUT /v2/<name>/manifests/<reference>` - Push manifest
- `GET /v2/<name>/blobs/<digest>` - Get blob
- `HEAD /v2/<name>/blobs/<digest>` - Check blob exists
- `POST /v2/<name>/blobs/uploads/` - Start upload
- `PATCH /v2/<name>/blobs/uploads/<session_id>` - Upload chunk
- `PUT /v2/<name>/blobs/uploads/<session_id>?digest=` - Complete upload

### Management API
- `POST /api/v1/auth/login` - Authenticate and get JWT token
- `GET /api/v1/users` - List users
- `POST /api/v1/users` - Create user
- `GET /api/v1/users/<id>` - Get user
- `PUT /api/v1/users/<id>` - Update user
- `DELETE /api/v1/users/<id>` - Delete user
- `GET /api/v1/cache/stats` - Get cache statistics
- `DELETE /api/v1/cache` - Clear cache
- `POST /api/v1/cache/cleanup` - Run cache cleanup

## Test Environment

A local Harbor instance is available for development and testing in `harbor-setup/`.

### Starting Harbor

```bash
cd harbor-setup/harbor
docker compose up -d
```

### Stopping Harbor

```bash
cd harbor-setup/harbor
docker compose down
```

### Harbor Access

- **URL**: http://localhost:8880
- **Username**: admin
- **Password**: Harbor12345

### Notes

- Uses ARM64-compatible images from `ghcr.io/octohelm/harbor` (v2.14.0)
- Data is stored in `harbor-setup/data/`
- Logs are stored in `harbor-setup/logs/`

## End-to-End Testing

```bash
# Run all tests
./tests/e2e-test.sh

# Run specific test suite
./tests/e2e-test.sh basic      # Basic health, auth, stats tests
./tests/e2e-test.sh multiarch  # OCI multi-architecture tests
./tests/e2e-test.sh pull       # Pull operation tests
./tests/e2e-test.sh cache      # Cache management tests

# Setup multi-arch test images (requires skopeo)
./tests/setup-multiarch.sh
```

## Docker

```bash
# Build Docker image
docker build -t harbor-cache .

# Run with Docker Compose
docker compose up -d

# Run with S3 storage (MinIO)
docker compose --profile s3 up -d
```

## Programming rules
- At the end of every implementation cycle, verify if the feature is working with the test cluster. Repeat the test cycle until the bugs are sorted out. After that, commit the changes to local git repository.
