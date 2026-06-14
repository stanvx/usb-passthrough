# Changelog

All notable changes to the AnyPlug Home Assistant add-on are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-06-14

### Added

- Initial Home Assistant add-on packaging for AnyPlug USB/IP server
- Docker-based add-on with multi-stage build (Rust → HA base image)
- Configuration schema supporting:
  - `port` — USB/IP server listen port (default: 3240)
  - `metrics_port` — API/metrics endpoint (0 = disabled, reserved for future release)
  - `encryption` — AES-256-GCM toggle
  - `allowlist` — device VID:PID allowlist
- `run.sh` entrypoint reads `/data/options.json` and launches `usbip-server` with the correct flags
- Host network mode for mDNS/USB discovery
- USB and privileged capabilities for device access
- README with configuration reference and troubleshooting guide
