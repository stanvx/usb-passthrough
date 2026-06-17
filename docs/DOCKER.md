# Docker Deployment

AnyPlug Server is available as a multi-architecture Docker container. Targets:

| Architecture | Use Case |
|--------------|----------|
| `amd64` | x86_64 servers, cloud VMs, Intel NUCs |
| `arm64` | Raspberry Pi 4/5, Apple Silicon, ARM servers |
| `armv7` | Raspberry Pi 2/3, older ARM NAS devices |

## Quick Start

```bash
# Pull and run
docker run -d \
  --name anyplug-server \
  --network host \
  --privileged \
  -v /dev/bus/usb:/dev/bus/usb \
  -p 3240:3240 \
  ghcr.io/stanvx/anyplug-server:latest

# With environment variables
docker run -d \
  --name anyplug-server \
  --network host \
  --privileged \
  -v /dev/bus/usb:/dev/bus/usb \
  -e USBIP_ALLOWED_DEVICES="046d:c261,046d:c29b" \
  -e USBIP_ENCRYPTION=true \
  -p 3240:3240 \
  ghcr.io/stanvx/anyplug-server:latest
```

## Docker Compose

```bash
docker-compose up -d
```

Edit `docker-compose.yml` to set allowed devices or enable encryption.

## Configuration

The server reads configuration from these sources (highest priority first):

1. **Environment variables**
2. **Mounted config file** (`/etc/anyplug/config.toml`)

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `USBIP_BIND_ADDRESS` | `0.0.0.0` | Address to bind the server socket |
| `USBIP_PORT` | `3240` | TCP port for USB/IP traffic — see [PORTS.md](../docs/api/PORTS.md) for the full port model |
| `USBIP_ALLOWED_DEVICES` | (empty — allow all) | Comma-separated VID:PID pairs |
| `USBIP_ENCRYPTION` | `false` | Enable AES-256-GCM encryption |

### Config File

When mounting a `config.toml` at `/etc/anyplug/config.toml`:

```toml
bind_address = "0.0.0.0"
port = 3240
encryption_enabled = false

[[allowed_vid_pid]]
vid = 0x046d
pid = 0xc261
```

## Building Locally

```bash
# Build for your native architecture
docker build -t anyplug-server .

# Build for a specific platform
docker build --platform linux/arm64 -t anyplug-server:arm64 .

# Multi-arch build (requires buildx)
docker buildx build \
  --platform linux/amd64,linux/arm64,linux/arm/v7 \
  -t anyplug-server:latest \
  --push .
```

## NAS Deployments

### Synology (DSM)

```bash
# On DSM: Docker → Registry → search "anyplug-server"
# Or via CLI:
docker run -d \
  --name anyplug-server \
  --network host \
  --privileged \
  -v /dev/bus/usb:/dev/bus/usb \
  ghcr.io/stanvx/anyplug-server:latest
```

### QNAP

```bash
# Container Station → Create → search "anyplug-server"
# Set network mode to Host, enable Privileged mode
```

### Raspberry Pi OS

```bash
# Pull the ARM64 image
docker pull ghcr.io/stanvx/anyplug-server:latest

# Run with host networking for mDNS support
docker run -d \
  --name anyplug-server \
  --network host \
  --privileged \
  -v /dev/bus/usb:/dev/bus/usb \
  --restart unless-stopped \
  ghcr.io/stanvx/anyplug-server:latest
```

## Notes

- **Privileged mode is required** for USB device access. For production, consider a device-specific cgroup allowlist instead.
- **Host networking** simplifies mDNS advertisement. If using bridge mode, mDNS may not cross the bridge — clients will need the server's host IP.
- The container expects `/dev/bus/usb` to be available. On systems without USB (cloud VMs), the server will start but find no devices to export.
