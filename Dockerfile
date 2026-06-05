# ==========================================
# Stage 1: Builder
# ==========================================
FROM rust:1.80-slim-bookworm AS builder

WORKDIR /usr/src/helheim

# Install standard build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy the entire workspace
COPY . .

# Argument to optionally enable the CUDA feature for the Motor Cortex
# For GPU build: --build-arg CARGO_FEATURES="--features helheim-core/cuda"
ARG CARGO_FEATURES=""

# Build the gateway
RUN cargo build --release -p helheim-gateway $CARGO_FEATURES

# ==========================================
# Stage 2: Runtime
# ==========================================
# Use a build argument to switch the base image:
# - CPU (default): debian:bookworm-slim
# - GPU (Nvidia): nvidia/cuda:12.2.0-base-ubuntu22.04
ARG BASE_IMAGE=debian:bookworm-slim
FROM ${BASE_IMAGE}

WORKDIR /app

# Install runtime dependencies (libssl and certificates)
# Both debian:bookworm and ubuntu:22.04 support libssl3
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/helheim/target/release/helheim-gateway /app/helheim-gateway

# Copy the Starfield Dashboard static files
COPY helheim-dashboard /app/helheim-dashboard

# Configure environment variables
# Tell the gateway where to find the static files
ENV HELHEIM_DASHBOARD_DIR="/app/helheim-dashboard"
ENV RUST_LOG="helheim_gateway=info,axum=info"

EXPOSE 8080

# Execute the gateway
CMD ["/app/helheim-gateway"]
