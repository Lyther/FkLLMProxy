# Use a Rust base image
FROM rust:1.91-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app
COPY . .

# Build the binary
RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

# Install OpenSSL/CA certificates (needed for HTTPS)
RUN apt-get update && apt-get install -y ca-certificates libssl-dev && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/vertex-bridge /usr/local/bin/vertex-bridge

# Set environment variables
ENV APP_SERVER__HOST=0.0.0.0
ENV APP_SERVER__PORT=4000

EXPOSE 4000

CMD ["vertex-bridge"]
