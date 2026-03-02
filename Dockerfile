# syntax=docker/dockerfile:1

# ============================================================================
# ferroq — High-performance QQ Bot unified gateway
# Multi-stage build for minimal image size (~20MB)
# ============================================================================

# Stage 1: Build
FROM rust:1.85-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev

WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ferroq-core/Cargo.toml crates/ferroq-core/
COPY crates/ferroq-gateway/Cargo.toml crates/ferroq-gateway/
COPY crates/ferroq-web/Cargo.toml crates/ferroq-web/
COPY crates/ferroq/Cargo.toml crates/ferroq/

# Create dummy source files for dependency caching
RUN mkdir -p crates/ferroq-core/src crates/ferroq-gateway/src crates/ferroq-web/src crates/ferroq/src && \
    echo "pub fn dummy() {}" > crates/ferroq-core/src/lib.rs && \
    echo "pub fn dummy() {}" > crates/ferroq-gateway/src/lib.rs && \
    echo "pub fn dummy() {}" > crates/ferroq-web/src/lib.rs && \
    echo "fn main() {}" > crates/ferroq/src/main.rs

# Build dependencies only (cached layer)
RUN cargo build --release --package ferroq 2>/dev/null || true

# Copy actual source code
COPY crates/ crates/

# Touch source files to invalidate cache and rebuild
RUN touch crates/ferroq-core/src/lib.rs \
          crates/ferroq-gateway/src/lib.rs \
          crates/ferroq-web/src/lib.rs \
          crates/ferroq/src/main.rs

# Build the release binary
RUN cargo build --release --package ferroq

# Stage 2: Runtime
FROM alpine:3.20

# Install runtime dependencies (CA certificates for HTTPS)
RUN apk add --no-cache ca-certificates tzdata

# Create non-root user
RUN addgroup -S ferroq && adduser -S ferroq -G ferroq

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/ferroq /usr/local/bin/ferroq

# Copy default config
COPY config.example.yaml /app/config.example.yaml

# Create data directory
RUN mkdir -p /app/data && chown -R ferroq:ferroq /app

USER ferroq

# Default config location
ENV FERROQ_CONFIG=/app/config.yaml

EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8080/health || exit 1

ENTRYPOINT ["ferroq"]
CMD ["--config", "/app/config.yaml"]
