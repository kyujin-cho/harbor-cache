# Harbor Cache Dockerfile
#
# Build: docker build -t harbor-cache .
# Run:   docker run -p 5001:5001 -v ./config:/app/config -v ./data:/app/data harbor-cache

# Frontend build stage
FROM node:20-slim AS frontend-builder

WORKDIR /build/frontend

COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci

COPY frontend ./
RUN npm run build

# Backend build stage
FROM rust:1-slim AS builder

WORKDIR /build

# Install build dependencies (cmake needed for aws-lc-sys)
RUN apt-get update && apt-get install -y \
    pkg-config \
    cmake \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /build/target/release/harbor-cache /app/

# Copy default configuration
COPY config/default.toml /app/config/default.toml

# Copy frontend static files from frontend builder
COPY --from=frontend-builder /build/static /app/static

# Create data directory
RUN mkdir -p /app/data

# Expose port
EXPOSE 5001

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:5001/health || exit 1

# Default command
ENTRYPOINT ["/app/harbor-cache"]
CMD ["--config", "/app/config/default.toml"]
