# ── Stage 1: Build ───────────────────────────────────────────────────────────
FROM rust:1.91-alpine AS builder

RUN apk add --no-cache musl-dev libusb-dev pkgconfig

WORKDIR /app
COPY . .

# Build only the server binary (static linking for minimal runtime)
RUN cargo build --release -p usbip-server --target-dir /app/target && \
    cp /app/target/release/usbip-server /usr/local/bin/

# ── Stage 2: Runtime ─────────────────────────────────────────────────────────
FROM alpine:3.22

RUN apk add --no-cache libusb

COPY --from=builder /usr/local/bin/usbip-server /usr/local/bin/usbip-server

EXPOSE 3240/tcp

ENV USBIP_BIND_ADDRESS=0.0.0.0
ENV USBIP_PORT=3240
ENV USBIP_ENCRYPTION=false

ENTRYPOINT ["usbip-server"]
CMD ["--bind", "0.0.0.0", "--port", "3240"]
