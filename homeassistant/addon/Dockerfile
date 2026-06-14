# =============================================================================
# AnyPlug Server — Home Assistant Add-on Dockerfile
# Multi-stage: build Rust binary, package into HA base image.
# =============================================================================

# ---- Build stage ----
FROM rust:1.86-slim-bookworm AS builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        libusb-1.0-0-dev \
        pkg-config \
        cmake \
        && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Dependency cache layer
COPY Cargo.toml Cargo.lock ./
COPY shared/usbip-core/Cargo.toml shared/usbip-core/
COPY server/usbip-server/Cargo.toml server/usbip-server/
RUN mkdir -p shared/usbip-core/src server/usbip-server/src && \
    echo "fn main() {}" > server/usbip-server/src/main.rs && \
    touch shared/usbip-core/src/lib.rs && \
    cargo fetch

# Full source and build
COPY . .
RUN cargo build --release -p usbip-server && \
    cp target/release/usbip-server /out/usbip-server

# ---- Runtime stage ----
ARG BUILD_FROM
FROM $BUILD_FROM

RUN apk add --no-cache \
     libusb \
     ca-certificates \
     tzdata \
     bash \
     jq

COPY --from=builder /out/usbip-server /usr/local/bin/usbip-server

# Add-on entrypoint
COPY homeassistant/addon/run.sh /run.sh
RUN chmod +x /run.sh

WORKDIR /data

ENTRYPOINT ["/run.sh"]
