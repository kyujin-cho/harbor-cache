# Harbor Cache Troubleshooting Guide

This guide helps you diagnose and resolve common issues with Harbor Cache.

## Table of Contents

- [Quick Diagnostics](#quick-diagnostics)
- [Common Issues](#common-issues)
- [Debug Logging](#debug-logging)
- [Decision Trees](#decision-trees)
- [Error Reference](#error-reference)
- [FAQ](#faq)

## Quick Diagnostics

### Health Check

Start by verifying basic health:

```bash
# Check if Harbor Cache is running
curl -s http://localhost:5001/health | jq
```

Expected response:
```json
{
  "status": "healthy",
  "version": "0.1.0"
}
```

### System Status Script

Run this script to gather diagnostic information:

```bash
#!/bin/bash
echo "=== Harbor Cache Diagnostics ==="
echo ""

echo "1. Health Check:"
curl -s http://localhost:5001/health | jq . 2>/dev/null || echo "FAILED: Cannot reach health endpoint"
echo ""

echo "2. Registry API:"
curl -s http://localhost:5001/v2/ | jq . 2>/dev/null || echo "FAILED: Cannot reach registry API"
echo ""

echo "3. Metrics Sample:"
curl -s http://localhost:5001/metrics | head -20 2>/dev/null || echo "FAILED: Cannot reach metrics"
echo ""

echo "4. Process Status:"
pgrep -a harbor-cache || echo "No harbor-cache process found"
echo ""

echo "5. Port Binding:"
ss -tlnp | grep 5001 || netstat -tlnp | grep 5001 2>/dev/null || echo "Port 5001 not bound"
echo ""

echo "6. Disk Space:"
df -h /var/lib/harbor-cache 2>/dev/null || df -h .
echo ""

echo "7. Recent Logs (last 10 lines):"
journalctl -u harbor-cache -n 10 --no-pager 2>/dev/null || tail -10 /var/log/harbor-cache/*.log 2>/dev/null || echo "No logs found"
```

## Common Issues

### Cannot Start Harbor Cache

**Symptom:** Service fails to start or exits immediately.

**Possible Causes and Solutions:**

1. **Port already in use:**
   ```bash
   # Check what's using the port
   lsof -i :5001

   # Solution: Stop the other process or change port
   # In config.toml:
   # [server]
   # port = 5002
   ```

2. **Invalid configuration:**
   ```bash
   # Validate TOML syntax
   cat config.toml | python3 -c "import sys, toml; toml.load(sys.stdin)"

   # Check for common issues:
   # - Missing required fields
   # - Invalid file paths
   # - Incorrect data types
   ```

3. **Permission denied:**
   ```bash
   # Harbor Cache auto-creates the database and storage directories on startup.
   # If you get permission errors, ensure the process user has write access
   # to the parent paths configured for database.path and storage.local.path.
   ls -la /var/lib/harbor-cache/

   # Fix permissions
   chown -R harbor-cache:harbor-cache /var/lib/harbor-cache/
   chmod 755 /var/lib/harbor-cache/
   ```

4. **Database locked:**
   ```bash
   # Check for lock files
   ls -la /var/lib/harbor-cache/*.db*

   # Remove stale locks (only if process is stopped!)
   rm /var/lib/harbor-cache/*.db-wal
   rm /var/lib/harbor-cache/*.db-shm
   ```

5. **TLS certificate issues:**
   ```bash
   # Verify certificate is readable
   openssl x509 -in /path/to/cert.pem -text -noout

   # Verify key matches certificate
   openssl x509 -noout -modulus -in cert.pem | md5sum
   openssl rsa -noout -modulus -in key.pem | md5sum
   # (Both should match)
   ```

### Cannot Connect to Harbor Cache

**Symptom:** Connection refused or timeout when accessing Harbor Cache.

**Diagnostic Steps:**

1. **Check if service is running:**
   ```bash
   systemctl status harbor-cache
   # or
   docker ps | grep harbor-cache
   ```

2. **Check binding:**
   ```bash
   # Verify it's listening
   ss -tlnp | grep 5001

   # If bound to 127.0.0.1, change to 0.0.0.0 in config
   ```

3. **Check firewall:**
   ```bash
   # List firewall rules
   ufw status
   # or
   iptables -L -n | grep 5001

   # Allow port
   ufw allow 5001/tcp
   ```

4. **Check Docker networking (if containerized):**
   ```bash
   # Verify port mapping
   docker port harbor-cache

   # Check network
   docker network inspect bridge
   ```

### Cannot Pull Images

**Symptom:** `docker pull` fails with various errors.

**Error: "manifest unknown"**

```
Error response from daemon: manifest for localhost:5001/library/nginx:latest not found
```

**Causes:**
- Image doesn't exist in upstream registry
- Repository path is incorrect

**Solutions:**
```bash
# Verify image exists in upstream Harbor
curl -u admin:password https://harbor.example.com/v2/library/nginx/tags/list

# Check you're using the correct path
# Format: <cache>/<project>/<image>:<tag>
docker pull localhost:5001/library/nginx:latest
```

**Error: "unauthorized"**

```
Error response from daemon: Get "https://localhost:5001/v2/": unauthorized
```

**Causes:**
- Authentication required but not provided
- Invalid credentials
- Token expired

**Solutions:**
```bash
# Login to Harbor Cache
docker login localhost:5001

# Verify credentials work
curl -X POST http://localhost:5001/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin"}'
```

**Error: "connection refused" or "connection reset"**

**Causes:**
- Harbor Cache not running
- TLS mismatch (HTTP vs HTTPS)
- Firewall blocking

**Solutions:**
```bash
# Check service status
systemctl status harbor-cache

# If using HTTP, add to insecure registries
# /etc/docker/daemon.json
{
  "insecure-registries": ["localhost:5001"]
}

# Restart Docker
systemctl restart docker
```

### Upstream Connection Failures

**Symptom:** Cache misses always fail; cannot fetch from upstream.

**Diagnostic:**
```bash
# Enable debug logging
RUST_LOG=debug ./harbor-cache --config config.toml

# Test upstream connectivity manually
curl -v https://harbor.example.com/v2/
```

**Causes and Solutions:**

1. **DNS resolution failure:**
   ```bash
   # Test DNS
   dig harbor.example.com
   nslookup harbor.example.com

   # If DNS fails, add to /etc/hosts
   10.0.0.100 harbor.example.com
   ```

2. **TLS certificate validation:**
   ```bash
   # Test TLS
   openssl s_client -connect harbor.example.com:443

   # If self-signed, set skip_tls_verify = true in config
   # (Not recommended for production)
   ```

3. **Authentication failure:**
   ```bash
   # Test upstream credentials
   curl -u username:password https://harbor.example.com/v2/

   # Check credentials in config.toml
   ```

4. **Network/firewall issues:**
   ```bash
   # Test connectivity
   telnet harbor.example.com 443
   nc -zv harbor.example.com 443

   # Check proxy settings if applicable
   echo $HTTP_PROXY $HTTPS_PROXY
   ```

### Slow Performance

**Symptom:** Image pulls are slower than expected.

**Diagnostic:**
```bash
# Check cache hit rate
curl -s http://localhost:5001/api/v1/cache/stats \
  -H "Authorization: Bearer $TOKEN" | jq '.hit_rate'

# Check disk I/O
iostat -x 1 5

# Check network
iftop -i eth0
```

**Causes and Solutions:**

1. **Low cache hit rate:**
   - Increase cache size
   - Increase retention period
   - Pre-warm cache with common images

2. **Slow disk I/O:**
   - Move to SSD storage
   - Check for disk space issues
   - Optimize database: `sqlite3 db.db "VACUUM;"`

3. **Network bottleneck:**
   - Check bandwidth to upstream
   - Consider edge deployment
   - Enable keep-alive connections

4. **Large images:**
   - Normal for first pull (cache miss)
   - Verify subsequent pulls are faster (cache hit)

### Database Errors

**Symptom:** Errors mentioning SQLite or database.

**Note:** Harbor Cache automatically creates the database parent directory on startup. If you previously encountered "unable to open database file" errors due to a missing directory, this is now handled automatically. Permission errors can still occur if the process does not have write access to the parent path.

**Common Errors:**

1. **"database is locked"**
   ```bash
   # Only one process should access the database
   fuser /var/lib/harbor-cache/harbor-cache.db

   # Increase busy timeout (if supported)
   # Or restart the service
   systemctl restart harbor-cache
   ```

2. **"database disk image is malformed"**
   ```bash
   # Database corruption - restore from backup
   cp /backup/harbor-cache.db /var/lib/harbor-cache/harbor-cache.db

   # Or rebuild (loses stats/users)
   rm /var/lib/harbor-cache/harbor-cache.db
   systemctl restart harbor-cache
   ```

3. **"SQLITE_FULL: database or disk is full"**
   ```bash
   # Check disk space
   df -h /var/lib/harbor-cache/

   # Clean up space
   # - Clear cache
   # - Remove old logs
   # - Increase storage
   ```

## Debug Logging

### Enabling Debug Logs

**Environment variable:**
```bash
RUST_LOG=debug ./harbor-cache --config config.toml
```

**Fine-grained control:**
```bash
# All crates at debug
RUST_LOG=debug

# Specific crate at debug
RUST_LOG=harbor_core=debug

# Multiple settings
RUST_LOG=harbor_core=debug,harbor_api=trace,harbor_proxy=info
```

**Log levels (least to most verbose):**
- `error` - Only errors
- `warn` - Warnings and errors
- `info` - General information (default)
- `debug` - Detailed debugging
- `trace` - Very verbose, includes request/response bodies

### Reading Logs

**Systemd:**
```bash
# Follow logs
journalctl -u harbor-cache -f

# Last 100 lines
journalctl -u harbor-cache -n 100

# Since boot
journalctl -u harbor-cache -b

# Specific time range
journalctl -u harbor-cache --since "2024-01-15 10:00" --until "2024-01-15 11:00"
```

**Docker:**
```bash
# Follow logs
docker logs -f harbor-cache

# Last 100 lines
docker logs --tail 100 harbor-cache

# With timestamps
docker logs -t harbor-cache
```

### Log Analysis

**Find errors:**
```bash
journalctl -u harbor-cache | grep -i error
journalctl -u harbor-cache | grep -i "level.*ERROR"
```

**Find slow requests:**
```bash
# JSON logs - find requests over 1 second
journalctl -u harbor-cache -o json | jq 'select(.duration_ms > 1000)'
```

**Count by status:**
```bash
# Count HTTP status codes
journalctl -u harbor-cache | grep -oP 'status=\K\d+' | sort | uniq -c
```

## Decision Trees

### Cannot Pull Images

```
Cannot pull image
│
├── Health check fails?
│   ├── Yes → See "Cannot Start Harbor Cache"
│   └── No ↓
│
├── Error: "unauthorized"?
│   ├── Yes → docker login and verify credentials
│   └── No ↓
│
├── Error: "manifest unknown"?
│   ├── Yes → Verify image exists in upstream
│   └── No ↓
│
├── Error: "connection refused"?
│   ├── Yes → Check binding and firewall
│   └── No ↓
│
├── Error: "TLS"/"certificate"?
│   ├── Yes → Add to insecure-registries or fix certs
│   └── No ↓
│
└── Enable debug logging and check upstream connectivity
```

### Slow Performance

```
Slow image pulls
│
├── First pull or repeat pull?
│   ├── First pull (cache miss) → Normal, fetching from upstream
│   └── Repeat pull ↓
│
├── Cache hit rate < 70%?
│   ├── Yes → Increase cache size or retention
│   └── No ↓
│
├── High disk I/O wait?
│   ├── Yes → Move to SSD or optimize database
│   └── No ↓
│
├── Network bottleneck?
│   ├── Yes → Check upstream bandwidth
│   └── No ↓
│
└── Check CPU and memory usage
```

### Service Won't Start

```
Service fails to start
│
├── Port already in use?
│   ├── Yes → Stop other service or change port
│   └── No ↓
│
├── Configuration syntax error?
│   ├── Yes → Fix TOML syntax
│   └── No ↓
│
├── Permission denied errors?
│   ├── Yes → Fix directory permissions
│   └── No ↓
│
├── TLS certificate problems?
│   ├── Yes → Verify cert/key paths and format
│   └── No ↓
│
├── Database errors?
│   ├── Yes → Check DB file permissions or restore backup
│   └── No ↓
│
└── Enable debug logging for more details
```

## Error Reference

### OCI Distribution Errors

| Code | HTTP Status | Meaning | Solution |
|------|-------------|---------|----------|
| `UNAUTHORIZED` | 401 | Invalid or missing credentials | Login with valid credentials |
| `DENIED` | 403 | Permission denied | Check user role |
| `NAME_UNKNOWN` | 404 | Repository not found | Verify repository name |
| `MANIFEST_UNKNOWN` | 404 | Manifest not found | Verify tag/digest exists |
| `BLOB_UNKNOWN` | 404 | Blob not found | Layer may have been garbage collected |
| `DIGEST_INVALID` | 400 | Digest format invalid | Use correct sha256:... format |
| `SIZE_INVALID` | 400 | Content size mismatch | Retry the upload |
| `UNSUPPORTED` | 415 | Unsupported media type | Check Accept headers |

### Internal Errors

| Error Message | Cause | Solution |
|---------------|-------|----------|
| "database is locked" | Concurrent access | Restart service |
| "connection refused" | Upstream unreachable | Check network/firewall |
| "certificate verify failed" | TLS trust issue | Add CA cert or skip verify |
| "no such file or directory" | Missing config file | Check paths in config (database and storage directories are auto-created) |
| "permission denied" | File access issue | Fix permissions |
| "address already in use" | Port conflict | Change port or stop other service |

## FAQ

### General

**Q: How do I know if caching is working?**

A: Check the hit rate in cache stats:
```bash
curl -s http://localhost:5001/api/v1/cache/stats \
  -H "Authorization: Bearer $TOKEN" | jq
```
A hit rate above 0 indicates caching is working. For repeat pulls, the rate should be high.

**Q: How long does it take for the cache to warm up?**

A: The cache warms up as images are pulled. For a typical development team, expect:
- 1 week: Core base images cached
- 1 month: Most common images cached
- Hit rate stabilizes around 60-80%

**Q: Can I pre-warm the cache?**

A: Yes, pull your common images during off-peak hours:
```bash
for image in nginx:latest redis:7 postgres:16; do
  docker pull localhost:5001/library/$image
done
```

### Configuration

**Q: What's the recommended cache size?**

A: Estimate: (Number of unique images) x (Average image size) x 1.5

Examples:
- Small team (50 images, 500MB avg): 50 GB
- Medium org (200 images, 1GB avg): 300 GB
- Large enterprise: 1 TB+

**Q: Which eviction policy should I use?**

A: LRU (Least Recently Used) works best for most use cases. Use LFU if you have clearly "hot" images that should never be evicted.

**Q: How do I migrate from local to S3 storage?**

A:
1. Stop Harbor Cache
2. Update config to use S3
3. Start Harbor Cache
4. Cache will rebuild from upstream (or copy blobs manually)

### Performance

**Q: Why is the first pull slow?**

A: First pulls are cache misses - the image must be fetched from the upstream registry. Subsequent pulls should be much faster.

**Q: How can I improve hit rate?**

A:
- Increase cache size
- Increase retention period
- Ensure all users pull through the cache
- Use consistent image tags

**Q: My hit rate is 0%, what's wrong?**

A: Check:
1. Users are pulling through the cache (not direct)
2. Same images are being pulled multiple times
3. Cache hasn't been cleared recently
4. Retention period isn't too short

### Operations

**Q: How do I backup Harbor Cache?**

A: Backup the SQLite database:
```bash
sqlite3 /var/lib/harbor-cache/harbor-cache.db ".backup /backup/db.sqlite"
```
Cache data doesn't need backup - it rebuilds from upstream.

**Q: How do I update Harbor Cache?**

A:
1. Pull new version
2. Stop old instance
3. Start new instance
4. Verify health

For zero-downtime: Use multiple instances behind a load balancer.

**Q: How do I reset everything?**

A:
```bash
systemctl stop harbor-cache
rm -rf /var/lib/harbor-cache/*
systemctl start harbor-cache
```
This clears all cache and recreates the database with a new admin user.

### Security

**Q: I forgot the admin password, how do I reset it?**

A: Delete the database and restart - a new admin/admin account is created:
```bash
systemctl stop harbor-cache
rm /var/lib/harbor-cache/harbor-cache.db
systemctl start harbor-cache
# Login with admin/admin, then change password
```

**Q: Is it safe to use skip_tls_verify?**

A: Only for testing with self-signed certificates. In production, use proper certificates.

## Getting More Help

If this guide doesn't resolve your issue:

1. **Check existing issues:** [GitHub Issues](https://github.com/lablup/harbor-cache/issues)
2. **Enable debug logging** and collect relevant logs
3. **Open a new issue** with:
   - Harbor Cache version
   - Operating system
   - Configuration (redact secrets)
   - Full error messages
   - Steps to reproduce
