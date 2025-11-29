# Use a Rust base image
FROM rust:1.83-slim-bookworm as builder

WORKDIR /usr/src/app
COPY . .

# Build the binary
RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

# Install OpenSSL/CA certificates (needed for HTTPS)
RUN apt-get update && apt-get install -y ca-certificates libssl-dev && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/vertex-bridge /usr/local/bin/vertex-bridge
COPY vertex-bridge.toml /etc/vertex-bridge/vertex-bridge.toml

# Set environment variables
ENV APP_SERVER__HOST=0.0.0.0
ENV APP_SERVER__PORT=4000

EXPOSE 4000

CMD ["vertex-bridge"]
