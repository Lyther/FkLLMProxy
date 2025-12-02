# Multi-stage build for vertex-bridge (Rust)
# Pinned by digest for security
FROM rust@sha256:a45bf1f5d9af0a23b26703b3500d70af1abff7f984a7abef5a104b42c02a292b AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

# Copy dependency manifests first for better caching
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build the binary
RUN cargo build --release

# Runtime image
# Pinned by digest for security
FROM debian@sha256:b4aa902587c2e61ce789849cb54c332b0400fe27b1ee33af4669e1f7e7c3e22f

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 appuser && \
    mkdir -p /app && \
    chown -R appuser:appuser /app

WORKDIR /app

# Copy binary from builder
COPY --from=builder --chown=appuser:appuser /usr/src/app/target/release/vertex-bridge /usr/local/bin/vertex-bridge

# Set environment variables
ENV APP_SERVER__HOST=0.0.0.0
ENV APP_SERVER__PORT=4000

USER appuser

EXPOSE 4000

HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:4000/health || exit 1

CMD ["vertex-bridge"]
