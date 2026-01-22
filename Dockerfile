# Harbor Cache Dockerfile
#
# Build: docker build -t harbor-cache .
# Run:   docker run -p 5001:5001 -v ./config:/app/config -v ./data:/app/data harbor-cache

# Build stage
FROM rust:1.85-slim AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
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
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /build/target/release/harbor-cache /app/

# Copy default configuration
COPY config/default.toml /app/config/default.toml

# Copy frontend static files
COPY static /app/static

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
