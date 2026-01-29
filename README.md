# Harbor Cache

A lightweight caching proxy for Harbor container registries, written in Rust.

Harbor Cache acts as an intermediary between Docker clients and an upstream Harbor registry, caching container images and artifacts locally to reduce bandwidth usage and improve pull times.

## Features

- **OCI Distribution Spec Compliant**: Full support for pulling and pushing container images
- **Multi-Architecture Support**: Handles manifest lists and OCI image indexes
- **Multiple Upstreams**: Configure multiple upstream Harbor registries with route-based selection
- **Multiple Storage Backends**: Local disk or S3-compatible storage (AWS S3, MinIO)
- **Cache Management**: Configurable eviction policies (LRU, LFU, FIFO)
- **Web UI**: Dashboard for monitoring and management
- **REST API**: Full management API for automation
- **Authentication**: JWT-based auth with role-based access control
- **TLS/HTTPS Support**: Native TLS support with PEM certificates
- **Prometheus Metrics**: Built-in metrics endpoint for monitoring

## Documentation

| Document | Description |
|----------|-------------|
| [User Guide](docs/user-guide.md) | End-user tutorial for Docker configuration and common workflows |
| [Web UI Guide](docs/web-ui-guide.md) | Guide to the web-based management interface |
| [API Reference](docs/api-reference.md) | Complete REST API documentation |
| [Configuration](docs/configuration.md) | All configuration options explained |
| [Architecture](docs/architecture.md) | System design and crate structure |
| [Deployment](docs/deployment.md) | Deployment scenarios (Docker, Kubernetes, systemd) |
| [Production Guide](docs/production-guide.md) | Performance tuning, HA, backup, and monitoring |
| [Troubleshooting](docs/troubleshooting.md) | Common issues and solutions |
| [Security](SECURITY.md) | Security model and hardening guidelines |
| [Contributing](CONTRIBUTING.md) | Development setup and contribution guidelines |

## Quick Start

### Using Docker

```bash
# Build the image
docker build -t harbor-cache .

# Run with default configuration
docker run -p 5001:5001 \
  -v ./config:/app/config \
  -v ./data:/app/data \
  harbor-cache
```

### Using Docker Compose

```bash
docker compose up -d
```

### Building from Source

```bash
# Build
cargo build --release

# Run
./target/release/harbor-cache --config config/default.toml
```

## Configuration

Harbor Cache is configured via TOML files. See `config/default.toml` for all options.

### Key Configuration Options

```toml
[server]
bind_address = "0.0.0.0"
port = 5001

[cache]
max_size = 10737418240  # 10 GB
retention_days = 30
eviction_policy = "lru"  # lru, lfu, or fifo

# Multiple upstreams configuration
# (Legacy [upstream] format is also supported for backwards compatibility)
[[upstreams]]
name = "default"
display_name = "Default Harbor"
url = "https://harbor.example.com"
registry = "library"
username = "admin"
password = "secret"
skip_tls_verify = false
priority = 100
enabled = true
cache_isolation = "shared"  # or "isolated"
is_default = true

# Optional: Add route patterns for this upstream
# [[upstreams.routes]]
# pattern = "library/*"
# priority = 100

# Additional upstream example
# [[upstreams]]
# name = "team-a"
# display_name = "Team A Registry"
# url = "https://harbor2.example.com"
# registry = "team-a"
# priority = 50
# is_default = false

[storage]
backend = "local"  # local or s3

[storage.local]
path = "./data/cache"

[storage.s3]
# S3 configuration (used when backend = "s3")
# bucket = "harbor-cache"
# region = "us-east-1"
# endpoint = "http://localhost:9000"  # For MinIO or other S3-compatible services
# access_key = ""
# secret_key = ""
# prefix = ""  # Optional prefix for all objects
# allow_http = false  # Allow HTTP (not HTTPS) for MinIO local dev

[database]
path = "./data/harbor-cache.db"

[auth]
jwt_secret = "change-me-in-production"
enabled = true

[logging]
level = "info"     # trace, debug, info, warn, error
format = "pretty"  # pretty or json

[tls]
enabled = false
# cert_path = "/path/to/cert.pem"
# key_path = "/path/to/key.pem"
```

### TLS/HTTPS Configuration

To enable HTTPS:

```toml
[tls]
enabled = true
cert_path = "/etc/harbor-cache/tls/server.crt"
key_path = "/etc/harbor-cache/tls/server.key"
```

Generate a self-signed certificate for development:

```bash
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes \
  -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
```

## Usage

### Docker Client Configuration

Configure Docker to use Harbor Cache as a registry mirror:

```bash
# Pull through cache
docker pull localhost:5001/library/nginx:latest

# Push through cache (forwards to upstream)
docker tag myimage:latest localhost:5001/library/myimage:latest
docker push localhost:5001/library/myimage:latest
```

### Web UI

Access the management UI at `http://localhost:5001`

Default credentials: `admin` / `admin`

### API Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check |
| `GET /metrics` | Prometheus metrics |
| `GET /v2/` | OCI Distribution version check |
| `GET /v2/<name>/manifests/<ref>` | Pull manifest |
| `GET /v2/<name>/blobs/<digest>` | Pull blob |
| `POST /api/v1/auth/login` | Authenticate |
| `GET /api/v1/cache/stats` | Cache statistics |
| `DELETE /api/v1/cache` | Clear cache |
| `POST /api/v1/cache/cleanup` | Run cache cleanup |
| `GET /api/v1/users` | List users |
| `POST /api/v1/users` | Create user |
| `GET /api/v1/config` | Get configuration |
| `PUT /api/v1/config` | Update configuration |

### User Roles

- **admin**: Full access to all features
- **read-write**: Can pull and push images
- **read-only**: Can only pull images

## Development

### Prerequisites

- Rust 1.84+
- Node.js 18+ (for frontend)
- Docker (for testing)

### Building

```bash
# Backend
cargo build

# Frontend
cd frontend
npm install
npm run build
```

### Testing

```bash
# Start test Harbor instance
cd harbor-setup/harbor
docker compose up -d

# Run e2e tests
./tests/e2e-test.sh

# Run specific test suite
./tests/e2e-test.sh multiarch
```

### Project Structure

```
harbor-cache/
├── crates/
│   ├── harbor-cache/     # Main binary
│   ├── harbor-core/      # Core business logic
│   ├── harbor-storage/   # Storage backends
│   ├── harbor-proxy/     # Upstream client
│   ├── harbor-api/       # REST API routes
│   ├── harbor-auth/      # Authentication
│   └── harbor-db/        # Database layer
├── frontend/             # Vue.js web UI
├── config/               # Configuration files
├── static/               # Built frontend assets
└── tests/                # E2E tests
```

## Monitoring

### Prometheus Metrics

Metrics are exposed at `/metrics` in Prometheus format:

- `harbor_cache_health_checks_total` - Health check requests
- `harbor_cache_requests_total` - Total cache requests
- `harbor_cache_hits_total` - Cache hits
- `harbor_cache_misses_total` - Cache misses
- `harbor_cache_size_bytes` - Current cache size
- `harbor_cache_entries` - Number of cached entries

### Grafana Dashboard

Import the provided dashboard from `docs/grafana-dashboard.json` (coming soon).

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) before submitting PRs.

## Security

For security concerns, please see our [Security Policy](SECURITY.md). Do not open public issues for security vulnerabilities.
