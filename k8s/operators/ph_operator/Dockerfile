# Stage 1: Build the Rust application
FROM rust:1.78 as builder

WORKDIR /usr/src/ph-operator
COPY . .

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev

# Build the operator in release mode
RUN cargo build --release --bin ph-operator

# Stage 2: Create the final, minimal image
FROM debian:12-slim

# Install SSL certificates needed for HTTPS communication with the Kubernetes API
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/ph-operator/target/release/ph-operator /usr/local/bin/ph-operator

# Set the binary as the entrypoint
ENTRYPOINT ["/usr/local/bin/ph-operator"]
