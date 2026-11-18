# Build stage
FROM rust:1.83-slim-bookworm AS builder

WORKDIR /app

# Install dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy Cargo files
COPY Cargo.toml Cargo.lock ./

# Create src directory and copy source
RUN mkdir -p src plugins
COPY src/ ./src/
COPY plugins/ ./plugins/

# Build the application
RUN cargo build --release --bin rss-proxy

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy the built binary
COPY --from=builder /app/target/release/rss-proxy /usr/local/bin/rss-proxy

# Copy config and plugins
COPY config.yaml /app/config.yaml
COPY plugins/ /app/plugins/

EXPOSE 8080

# Run the proxy server
# Bind to 0.0.0.0 to accept connections from the host
CMD ["rss-proxy", "--host", "0.0.0.0", "--port", "8080", "--config", "/app/config.yaml"]
