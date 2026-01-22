# Security Policy

This document describes the security model of Harbor Cache and provides guidelines for secure deployment.

## Table of Contents

- [Security Model Overview](#security-model-overview)
- [Authentication and Authorization](#authentication-and-authorization)
- [TLS Configuration](#tls-configuration)
- [Secrets Management](#secrets-management)
- [Production Hardening Checklist](#production-hardening-checklist)
- [Reporting Security Issues](#reporting-security-issues)

## Security Model Overview

Harbor Cache implements a defense-in-depth security approach:

```
┌─────────────────────────────────────────────────────────────────┐
│                        Network Layer                             │
│  - TLS encryption for all traffic                               │
│  - Firewall rules limiting access                               │
└─────────────────────────────────────────────────────────────────┘
                                │
┌─────────────────────────────────────────────────────────────────┐
│                     Authentication Layer                         │
│  - JWT tokens for API access                                    │
│  - Argon2 password hashing                                      │
└─────────────────────────────────────────────────────────────────┘
                                │
┌─────────────────────────────────────────────────────────────────┐
│                     Authorization Layer                          │
│  - Role-based access control (RBAC)                             │
│  - Endpoint-level permission checks                              │
└─────────────────────────────────────────────────────────────────┘
                                │
┌─────────────────────────────────────────────────────────────────┐
│                      Application Layer                           │
│  - Input validation                                              │
│  - Digest verification for content integrity                    │
└─────────────────────────────────────────────────────────────────┘
```

### Key Security Features

| Feature | Implementation |
|---------|----------------|
| Transport Security | Native TLS support with PEM certificates |
| Password Storage | Argon2id hashing (memory-hard) |
| Token Authentication | JWT with HS256 signing |
| Access Control | Role-based (admin, read-write, read-only) |
| Content Verification | SHA-256 digest validation |

## Authentication and Authorization

### JWT Token Authentication

Harbor Cache uses JWT (JSON Web Tokens) for API authentication:

**Token Structure:**
```json
{
  "header": {
    "alg": "HS256",
    "typ": "JWT"
  },
  "payload": {
    "sub": "1",
    "username": "admin",
    "role": "admin",
    "exp": 1705329600,
    "iat": 1705243200
  }
}
```

**Token Lifecycle:**
- Tokens expire after 24 hours
- Tokens are stateless (no server-side session)
- Refresh by re-authenticating

**Best Practices:**
- Store tokens securely (not in localStorage for web apps)
- Don't include tokens in URLs
- Implement token refresh before expiration

### Password Security

**Argon2id Configuration:**
- Memory: 19 MiB
- Iterations: 2
- Parallelism: 1
- Salt: Random 16 bytes
- Output: 32 bytes

This configuration follows OWASP recommendations and provides resistance against:
- GPU-based attacks
- ASIC attacks
- Time-memory trade-off attacks

**Password Requirements:**
- Minimum 4 characters (configurable)
- No complexity requirements enforced (consider organizational policy)

### Role-Based Access Control

| Role | Pull Images | Push Images | Manage Cache | Manage Users | Manage Config |
|------|-------------|-------------|--------------|--------------|---------------|
| `read-only` | Yes | No | No | No | No |
| `read-write` | Yes | Yes | No | No | No |
| `admin` | Yes | Yes | Yes | Yes | Yes |

**Endpoint Protection:**

| Endpoint | Required Role |
|----------|---------------|
| `GET /v2/*` | Any authenticated |
| `PUT /v2/*` | read-write or admin |
| `GET /api/v1/cache/stats` | Any authenticated |
| `DELETE /api/v1/cache` | admin |
| `POST /api/v1/cache/cleanup` | admin |
| `GET /api/v1/users` | admin |
| `POST /api/v1/users` | admin |
| `GET /api/v1/config` | admin |

### Unauthenticated Endpoints

The following endpoints do not require authentication:

| Endpoint | Purpose |
|----------|---------|
| `GET /health` | Health checks |
| `GET /healthz` | Kubernetes probes |
| `GET /metrics` | Prometheus scraping |
| `POST /api/v1/auth/login` | Authentication |

## TLS Configuration

### Enabling TLS

**Configuration:**

```toml
[tls]
enabled = true
cert_path = "/etc/harbor-cache/tls/server.crt"
key_path = "/etc/harbor-cache/tls/server.key"
```

### Certificate Requirements

**Supported Formats:**
- Certificate: PEM format (X.509)
- Private Key: PEM format (PKCS#1, PKCS#8, or SEC1)

**Recommended Key Specifications:**
- RSA: 2048 bits minimum, 4096 bits recommended
- ECDSA: P-256 or P-384 curve

### Generating Certificates

**Self-Signed (Development Only):**

```bash
# Generate private key and self-signed certificate
openssl req -x509 -newkey rsa:4096 \
  -keyout key.pem -out cert.pem \
  -days 365 -nodes \
  -subj "/CN=harbor-cache.example.com" \
  -addext "subjectAltName=DNS:harbor-cache.example.com,DNS:localhost,IP:127.0.0.1"
```

**Certificate Signing Request (Production):**

```bash
# Generate private key
openssl genrsa -out key.pem 4096

# Generate CSR
openssl req -new -key key.pem -out csr.pem \
  -subj "/CN=harbor-cache.example.com/O=Your Organization"

# Add SAN extension (create san.cnf)
cat > san.cnf << EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req

[req_distinguished_name]

[v3_req]
subjectAltName = @alt_names

[alt_names]
DNS.1 = harbor-cache.example.com
DNS.2 = harbor-cache
IP.1 = 10.0.0.100
EOF

# Generate CSR with SAN
openssl req -new -key key.pem -out csr.pem \
  -subj "/CN=harbor-cache.example.com" \
  -config san.cnf
```

Submit the CSR to your Certificate Authority.

### TLS Best Practices

1. **Use certificates from a trusted CA** in production
2. **Include all hostnames** in Subject Alternative Names (SAN)
3. **Rotate certificates** before expiration (90 days recommended for Let's Encrypt)
4. **Protect private keys:**
   ```bash
   chmod 600 /etc/harbor-cache/tls/server.key
   chown harbor-cache:harbor-cache /etc/harbor-cache/tls/server.key
   ```
5. **Monitor certificate expiration** with alerts

### Client Configuration

**Docker with Self-Signed Certificates:**

```bash
# Copy CA certificate to Docker trust store
mkdir -p /etc/docker/certs.d/harbor-cache.example.com:5001
cp ca.crt /etc/docker/certs.d/harbor-cache.example.com:5001/ca.crt
```

**System-Wide Trust (Linux):**

```bash
# Debian/Ubuntu
cp ca.crt /usr/local/share/ca-certificates/harbor-cache.crt
update-ca-certificates

# RHEL/CentOS
cp ca.crt /etc/pki/ca-trust/source/anchors/harbor-cache.crt
update-ca-trust
```

## Secrets Management

### Sensitive Configuration Values

The following configuration values should be treated as secrets:

| Setting | Location | Description |
|---------|----------|-------------|
| `auth.jwt_secret` | config.toml | JWT signing key |
| `upstream.username` | config.toml | Harbor credentials |
| `upstream.password` | config.toml | Harbor credentials |
| `storage.s3.access_key` | config.toml | AWS credentials |
| `storage.s3.secret_key` | config.toml | AWS credentials |

### Secure Storage Options

**1. Environment Variables:**

```bash
export JWT_SECRET="your-secret-here"
export UPSTREAM_PASSWORD="harbor-password"
```

**2. Docker Secrets (Swarm):**

```yaml
version: '3.8'
services:
  harbor-cache:
    secrets:
      - jwt_secret
      - upstream_password

secrets:
  jwt_secret:
    external: true
  upstream_password:
    external: true
```

**3. Kubernetes Secrets:**

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: harbor-cache-secrets
type: Opaque
stringData:
  jwt-secret: "your-jwt-secret"
  upstream-password: "harbor-password"
---
apiVersion: apps/v1
kind: Deployment
spec:
  template:
    spec:
      containers:
      - name: harbor-cache
        env:
        - name: JWT_SECRET
          valueFrom:
            secretKeyRef:
              name: harbor-cache-secrets
              key: jwt-secret
```

**4. HashiCorp Vault:**

```bash
# Store secrets
vault kv put secret/harbor-cache \
  jwt_secret="your-jwt-secret" \
  upstream_password="harbor-password"

# Retrieve at runtime
vault kv get -field=jwt_secret secret/harbor-cache
```

### Generating Secure Secrets

**JWT Secret (32+ characters):**

```bash
# Using OpenSSL
openssl rand -base64 32

# Using /dev/urandom
head -c 32 /dev/urandom | base64

# Using Python
python3 -c "import secrets; print(secrets.token_urlsafe(32))"
```

**Never use:**
- Default values in production
- Predictable patterns
- Secrets in version control

## Production Hardening Checklist

### Pre-Deployment

- [ ] **Change default credentials**
  - Change admin password
  - Generate new JWT secret

- [ ] **Enable TLS**
  - Obtain valid certificates
  - Configure cert/key paths
  - Verify certificate chain

- [ ] **Secure configuration file**
  ```bash
  chmod 600 /etc/harbor-cache/config.toml
  chown harbor-cache:harbor-cache /etc/harbor-cache/config.toml
  ```

- [ ] **Remove sensitive data from version control**
  - Use `.gitignore` for config files
  - Use environment variables or secrets management

### Network Security

- [ ] **Firewall rules**
  ```bash
  # Allow only necessary ports
  ufw allow 5001/tcp  # Harbor Cache
  ufw deny all
  ```

- [ ] **Restrict management access**
  - Limit API access to internal networks
  - Use VPN for remote administration

- [ ] **Disable insecure protocols**
  - No HTTP in production (TLS only)
  - No skip_tls_verify for upstream

### Runtime Security

- [ ] **Run as non-root user**
  ```bash
  useradd -r -s /bin/false harbor-cache
  chown -R harbor-cache:harbor-cache /var/lib/harbor-cache
  ```

- [ ] **Enable systemd security features**
  ```ini
  [Service]
  NoNewPrivileges=yes
  ProtectSystem=strict
  ProtectHome=yes
  PrivateTmp=yes
  ReadWritePaths=/var/lib/harbor-cache
  ```

- [ ] **Container security (Docker)**
  ```yaml
  services:
    harbor-cache:
      security_opt:
        - no-new-privileges:true
      read_only: true
      tmpfs:
        - /tmp
      user: "1000:1000"
  ```

### Monitoring and Auditing

- [ ] **Enable logging**
  ```toml
  [logging]
  level = "info"
  format = "json"
  ```

- [ ] **Log retention**
  - Configure log rotation
  - Archive logs for audit trail

- [ ] **Monitor authentication**
  - Alert on failed login attempts
  - Track admin actions

### Regular Maintenance

- [ ] **Update regularly**
  - Apply security patches
  - Update dependencies

- [ ] **Rotate credentials**
  - JWT secret (annually)
  - User passwords (per policy)
  - Upstream credentials (per policy)

- [ ] **Review access**
  - Audit user accounts
  - Remove unused accounts
  - Verify role assignments

## Reporting Security Issues

### Responsible Disclosure

If you discover a security vulnerability in Harbor Cache, please report it responsibly:

1. **Do not** open a public GitHub issue
2. **Email** security concerns to the maintainers
3. **Include:**
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

### Response Timeline

| Stage | Timeline |
|-------|----------|
| Acknowledgment | 48 hours |
| Initial assessment | 1 week |
| Fix development | Depends on severity |
| Coordinated disclosure | After fix released |

### Security Updates

- Watch the GitHub repository for security releases
- Subscribe to security announcements
- Update promptly when patches are released

## Additional Resources

- [OWASP Authentication Best Practices](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)
- [NIST Password Guidelines](https://pages.nist.gov/800-63-3/sp800-63b.html)
- [Docker Security Best Practices](https://docs.docker.com/develop/security-best-practices/)
- [Kubernetes Security Best Practices](https://kubernetes.io/docs/concepts/security/overview/)
