# Harbor Cache User Guide

This guide covers how to use Harbor Cache as an end-user, from basic setup to advanced workflows.

## Table of Contents

- [Introduction](#introduction)
- [Getting Started](#getting-started)
- [Configuring Docker](#configuring-docker)
- [Pulling Images](#pulling-images)
- [Pushing Images](#pushing-images)
- [Monitoring Cache Performance](#monitoring-cache-performance)
- [Common Workflows](#common-workflows)
- [Best Practices](#best-practices)

## Introduction

Harbor Cache is a transparent caching proxy for Harbor container registries. It sits between your Docker client and an upstream Harbor registry, caching container images locally to:

- **Reduce bandwidth usage**: Images are downloaded once and served from cache
- **Improve pull times**: Cached images are served instantly from local storage
- **Increase reliability**: Cached images remain available even during upstream outages

## Getting Started

### Prerequisites

Before using Harbor Cache, ensure you have:

- Docker installed (version 20.10 or later)
- Network access to the Harbor Cache server
- Valid credentials (if authentication is enabled)

### Verifying Connection

Test that Harbor Cache is accessible:

```bash
# Health check
curl http://harbor-cache.example.com:5001/health
```

Expected response:

```json
{
  "status": "healthy",
  "version": "0.1.0"
}
```

### Testing Registry API

Verify the OCI Distribution API is working:

```bash
curl http://harbor-cache.example.com:5001/v2/
```

Expected response:

```json
{}
```

With a `Docker-Distribution-API-Version: registry/2.0` header.

## Configuring Docker

There are several ways to configure Docker to use Harbor Cache.

### Method 1: Direct Pull (Recommended)

The simplest approach is to pull images directly using the cache URL:

```bash
docker pull harbor-cache.example.com:5001/library/nginx:latest
```

This method requires no Docker daemon configuration changes.

### Method 2: Insecure Registry (HTTP)

If Harbor Cache is running without TLS, add it to Docker's insecure registries list.

**Linux/macOS:**

Edit `/etc/docker/daemon.json`:

```json
{
  "insecure-registries": ["harbor-cache.example.com:5001"]
}
```

Restart Docker:

```bash
sudo systemctl restart docker
```

**Docker Desktop (Windows/macOS):**

1. Open Docker Desktop Settings
2. Go to Docker Engine
3. Add to the JSON configuration:
   ```json
   {
     "insecure-registries": ["harbor-cache.example.com:5001"]
   }
   ```
4. Click "Apply & Restart"

### Method 3: TLS with Self-Signed Certificates

If Harbor Cache uses a self-signed TLS certificate:

```bash
# Create certificate directory
sudo mkdir -p /etc/docker/certs.d/harbor-cache.example.com:5001

# Copy the CA certificate
sudo cp ca.crt /etc/docker/certs.d/harbor-cache.example.com:5001/ca.crt
```

No Docker restart is required for certificate changes.

### Method 4: TLS with Trusted Certificates

If Harbor Cache uses certificates from a trusted CA (e.g., Let's Encrypt), no additional configuration is needed:

```bash
docker pull harbor-cache.example.com:5001/library/nginx:latest
```

## Pulling Images

### Basic Pull

Pull an image through the cache:

```bash
docker pull harbor-cache.example.com:5001/library/nginx:latest
```

The image path format is:

```
<cache-host>:<port>/<project>/<repository>:<tag>
```

### Pull by Digest

For reproducible builds, pull by digest:

```bash
docker pull harbor-cache.example.com:5001/library/nginx@sha256:abc123...
```

### Multi-Architecture Images

Harbor Cache fully supports multi-architecture images. Docker automatically selects the correct architecture:

```bash
# Pulls the manifest list and the appropriate platform manifest
docker pull harbor-cache.example.com:5001/library/alpine:latest
```

To pull a specific platform:

```bash
docker pull --platform linux/arm64 harbor-cache.example.com:5001/library/alpine:latest
```

### Understanding Cache Behavior

**First Pull (Cache Miss):**
1. Docker requests the image from Harbor Cache
2. Harbor Cache fetches from upstream Harbor
3. Image is stored in cache
4. Image is returned to Docker

**Subsequent Pulls (Cache Hit):**
1. Docker requests the image from Harbor Cache
2. Harbor Cache serves directly from cache
3. No upstream request is made

### Checking Cache Status

Use the API to verify an image is cached:

```bash
# Check if manifest exists (HEAD request)
curl -I http://harbor-cache.example.com:5001/v2/library/nginx/manifests/latest
```

A `200 OK` response indicates the manifest is cached.

## Pushing Images

Harbor Cache supports pushing images to the upstream registry.

### Tag and Push

```bash
# Tag your local image
docker tag myapp:latest harbor-cache.example.com:5001/library/myapp:latest

# Push through cache to upstream
docker push harbor-cache.example.com:5001/library/myapp:latest
```

### Push Flow

1. Docker initiates upload to Harbor Cache
2. Harbor Cache stores the blob locally
3. Harbor Cache forwards to upstream Harbor
4. Both locations now have the image

### Authentication for Push

Push operations typically require authentication. Use `docker login`:

```bash
docker login harbor-cache.example.com:5001
Username: your-username
Password: your-password
```

Credentials are stored in `~/.docker/config.json`.

## Monitoring Cache Performance

### Using the Web UI

Access the management UI at `http://harbor-cache.example.com:5001`

The Dashboard shows:
- Total cache size
- Number of cached entries (manifests and blobs)
- Cache hit rate
- Hit/miss counts

### Using the API

Get cache statistics programmatically:

```bash
# Login to get token
TOKEN=$(curl -s -X POST http://harbor-cache.example.com:5001/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin"}' | jq -r '.token')

# Get cache stats
curl -s http://harbor-cache.example.com:5001/api/v1/cache/stats \
  -H "Authorization: Bearer $TOKEN" | jq
```

Response:

```json
{
  "total_size": 1073741824,
  "total_size_human": "1.00 GB",
  "entry_count": 42,
  "manifest_count": 10,
  "blob_count": 32,
  "hit_count": 150,
  "miss_count": 50,
  "hit_rate": 0.75
}
```

### Prometheus Metrics

For production monitoring, scrape the `/metrics` endpoint:

```bash
curl http://harbor-cache.example.com:5001/metrics
```

Key metrics:
- `harbor_cache_hits_total`: Total cache hits
- `harbor_cache_misses_total`: Total cache misses
- `harbor_cache_size_bytes`: Current cache size
- `harbor_cache_entries`: Number of cached entries

## Common Workflows

### CI/CD Pipeline Integration

Configure your CI/CD system to use Harbor Cache:

**GitLab CI:**

```yaml
variables:
  DOCKER_HOST: tcp://docker:2375

before_script:
  - echo '{"insecure-registries":["harbor-cache.example.com:5001"]}' > /etc/docker/daemon.json
  - dockerd &

build:
  script:
    - docker pull harbor-cache.example.com:5001/library/node:18
    - docker build -t myapp .
```

**GitHub Actions:**

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - name: Configure Docker for cache
        run: |
          sudo mkdir -p /etc/docker
          echo '{"insecure-registries":["harbor-cache.example.com:5001"]}' | sudo tee /etc/docker/daemon.json
          sudo systemctl restart docker

      - name: Pull base image through cache
        run: docker pull harbor-cache.example.com:5001/library/python:3.11
```

### Kubernetes with Harbor Cache

Configure containerd to use Harbor Cache:

**/etc/containerd/config.toml:**

```toml
[plugins."io.containerd.grpc.v1.cri".registry]
  [plugins."io.containerd.grpc.v1.cri".registry.mirrors]
    [plugins."io.containerd.grpc.v1.cri".registry.mirrors."harbor-cache.example.com:5001"]
      endpoint = ["http://harbor-cache.example.com:5001"]
```

Or create a Kubernetes ImagePullSecret:

```bash
kubectl create secret docker-registry harbor-cache-secret \
  --docker-server=harbor-cache.example.com:5001 \
  --docker-username=your-username \
  --docker-password=your-password
```

### Development Workflow

For local development:

1. **Start Harbor Cache locally:**
   ```bash
   docker compose up -d
   ```

2. **Configure Docker Desktop:**
   Add `localhost:5001` to insecure registries

3. **Pull images through local cache:**
   ```bash
   docker pull localhost:5001/library/postgres:15
   ```

4. **Verify caching:**
   ```bash
   # First pull - cache miss (slower)
   time docker pull localhost:5001/library/redis:7

   # Remove local image
   docker rmi localhost:5001/library/redis:7

   # Second pull - cache hit (faster)
   time docker pull localhost:5001/library/redis:7
   ```

### Mirroring Strategy

For organizations with multiple sites:

```
                    ┌─────────────────┐
                    │  Harbor (HQ)    │
                    │  Production     │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
       ┌──────▼──────┐ ┌─────▼─────┐ ┌──────▼──────┐
       │ Cache (NYC) │ │Cache (LON)│ │ Cache (TKY) │
       └──────┬──────┘ └─────┬─────┘ └──────┬──────┘
              │              │              │
         Dev Team A      Dev Team B     Dev Team C
```

Each regional cache reduces cross-region bandwidth while maintaining access to the same images.

## Best Practices

### Image Tagging

1. **Use specific tags**: Prefer `nginx:1.25.3` over `nginx:latest`
2. **Use digests for production**: `nginx@sha256:...` ensures reproducibility
3. **Consistent naming**: Use the same image references across environments

### Cache Optimization

1. **Pre-warm the cache**: Pull frequently-used images before they're needed
   ```bash
   # Pre-warm script
   for image in nginx:1.25 redis:7 postgres:16; do
     docker pull harbor-cache.example.com:5001/library/$image
   done
   ```

2. **Monitor hit rates**: Aim for >70% hit rate in production
3. **Size appropriately**: Set cache size based on your image portfolio

### Security

1. **Use TLS in production**: Never use insecure registries in production
2. **Rotate credentials**: Change passwords periodically
3. **Least privilege**: Use read-only accounts for pull-only workloads

### Troubleshooting Common Issues

| Issue | Solution |
|-------|----------|
| `manifest unknown` | Image may not exist upstream; verify in Harbor |
| `unauthorized` | Check credentials and user permissions |
| `connection refused` | Verify Harbor Cache is running and accessible |
| Slow pulls | Check network connectivity to both cache and upstream |
| Cache not working | Verify cache stats show increasing hit count |

For detailed troubleshooting, see the [Troubleshooting Guide](troubleshooting.md).

## Next Steps

- [Web UI Guide](web-ui-guide.md) - Learn to use the management interface
- [API Reference](api-reference.md) - Automate with the REST API
- [Configuration](configuration.md) - Customize Harbor Cache settings
- [Production Guide](production-guide.md) - Deploy in production environments
