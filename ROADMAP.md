# USB/IP Passthrough — Development Roadmap

Project roadmap and milestone tracking for the USB/IP passthrough system.

---

## Legend

| Icon | Meaning |
|------|---------|
| ✅ | Done |
| 🔧 | In progress |
| 📅 | Planned |
| 💡 | Future idea |

---

## Milestone 1: Core Protocol ✅

- [x] USB/IP protocol v1.1.1 wire format (`shared/usbip-core/src/protocol.rs`)
- [x] URB types: CMD_SUBMIT, RET_SUBMIT, RET_UNLINK (`shared/usbip-core/src/urb.rs`)
- [x] Device descriptor enumeration
- [x] Device list (OP_REQ_DEVLIST / OP_REP_DEVLIST)
- [x] Device import (OP_REQ_IMPORT / OP_REP_IMPORT)
- [x] Big-endian wire encoding (UsbIpHeader, UsbIpDeviceEntry)
- [x] Maximum message size handling (1 MiB buffer)
- [x] Comprehensive unit tests for protocol types

## Milestone 2: Encryption ✅

- [x] AES-256-GCM encrypt/decrypt (`shared/usbip-core/src/crypto.rs`)
- [x] X25519 ECDH key exchange
- [x] Per-message nonce generation
- [x] HKDF-based session key derivation
- [x] `encrypt_usbip_message` / `decrypt_usbip_message` high-level API
- [x] Key pair generation and hex encoding
- [x] Test vectors (RFC 7748 Section 6.1)
- [x] JNI-friendly key management functions

## Milestone 3: mDNS Discovery ✅

- [x] Server mDNS advertisement (`_usbip._tcp.local.`) (`server/usbip-server/src/discovery.rs`)
- [x] Client mDNS browsing (`client/usbip-client/src/discovery.rs`)
- [x] 2-second discovery timeout
- [x] Service properties (version, platform)
- [x] Graceful shutdown of mDNS daemon

## Milestone 4: Linux Server ✅

- [x] libusb-based USB device enumeration (`server/usbip-server/src/usb.rs`)
- [x] Concurrent client handling (tokio async)
- [x] TCP listener on port 3240
- [x] Device export with optional VID:PID allowlist
- [x] Connection confirmation prompt
- [x] CLI with clap (`server/usbip-server/src/main.rs`)
- [x] `usbip-server list` command
- [x] Configurable bind address and port
- [x] Encryption enable/disable

## Milestone 5: Linux Client ✅

- [x] TCP connection to remote server (`client/usbip-client/src/client.rs`)
- [x] Device import protocol
- [x] VHCI kernel module integration (`client/usbip-client/src/vhci.rs`)
- [x] uinput fallback for non-VHCI systems
- [x] URB submission loop
- [x] CLI with clap (`client/usbip-client/src/main.rs`)
- [x] `--discover`, `--list`, `--connect` commands
- [x] TCP_NODELAY enabled

## Milestone 6: Windows Support ✅

- [x] Win32 SetupAPI USB enumeration (`windows/src/windows_usb.rs`)
- [x] Device VID/PID enumeration (G920-specific constants retired; caller-side detection)
- [x] egui system tray GUI (`windows/src/main.rs`)
- [x] Windows Service integration (`windows-service` crate)
- [x] Service install/start/stop commands
- [x] UAC-aware admin elevation
- [x] Firewall rule management
- [x] Portable and installer distribution

## Milestone 7: Android App ✅

- [x] Kotlin JNI bridge (`android/app/src/main/java/.../bridge/RustBridge.kt`)
- [x] Jetpack Compose UI (`android/app/src/main/java/.../ui/MainScreen.kt`)
- [x] Server mode with USB Host API (`UsbIpServer.kt`)
- [x] Client mode with TCP protocol (`UsbIpClient.kt`)
- [x] Foreground service with wake lock (`UsbPassthroughService.kt`)
- [x] mDNS discovery in UI
- [x] Rust compilation via rust-android-gradle plugin
- [x] Gradle build system

## Milestone 8: Android TV 🔧

- [x] TV-specific Compose UI (`android/tv/src/main/java/.../tv/TvMainActivity.kt`)
- [x] D-pad remote navigation
- [x] Foreground service on TV
- [x] Server and client modes
- [ ] Power management optimization for TV use
- [ ] Leanback UI extensions (headers, browse fragment)
- [ ] Google Play Store listing preparation
- [ ] TV input framework integration

## Milestone 9: Documentation ✅

- [x] README.md — project overview
- [x] ARCHITECTURE.md — system design
- [x] PROTOCOL.md — wire protocol reference
- [x] ROADMAP.md — milestone tracking
- [x] docs/SETUP.md — platform setup guides
- [x] docs/TROUBLESHOOTING.md — diagnosis and fixes
- [x] docs/G920-SPECIFIC.md — G920 deep dive
- [x] docs/ANDROID-TV.md — TV-specific guide
- [x] docs/PERFORMANCE.md — latency, benchmarks, tuning
- [x] docs/BUILDING.md — compilation from source

## Milestone 10: CI/CD Pipeline 🔧

- [x] GitHub Actions workflow
- [x] Rust test suite execution
- [x] Linux binary builds (x86_64)
- [x] Windows binary builds (x86_64)
- [ ] Android APK builds in CI
- [ ] Windows installer builds in CI
- [ ] Automated release creation
- [x] Multi-arch Linux builds (ARM64, ARMv7)

## Milestone 10b: Generality & Test Rig ✅

Device-class conformance testing that proves the project works with arbitrary USB devices,
not just the original reference hardware. Delivered per PRD #1.

- [x] Generic URB test scaffolding — HID IN, bulk OUT, control transfer round-trips
- [x] Descriptor fixture framework — TOML sidecar schema, G920 + HID keyboard starter corpus
- [x] QEMU kernel build + initramfs boot infrastructure (configfs + dummy_hcd/udc)
- [x] HID keyboard E2E tracer bullet (configfs gadget → usbip-server → usbip-client over loopback)
- [x] Multi-gadget E2E: mass-storage (file-backed LUN) + CDC-ACM (virtual serial) in single VM boot
- [x] Structured JSON test output per gadget (`test`, `status`, `duration_ms`)
- [x] E2E CI workflow (`.github/workflows/e2e-linux.yml`) with kernel caching + step summary
- [x] Architecture deepening — VHCI platform seam, busid parsing, parsed descriptors, URB executor seam
- [x] G920 debt fully retired — no device-specific constants in generic infrastructure
- [x] CONTRIBUTING.md fixture capture guide for community descriptor corpus

## Milestone 11: Performance Optimization 📅

- [ ] URB buffer pool sizing auto-tuning
- [ ] Zero-copy URB forwarding where possible
- [ ] Batch URB submission for high-throughput devices
- [ ] Memory-mapped VHCI ring buffer (Linux)
- [ ] Adaptive polling interval for Android
- [ ] TCP buffer auto-tuning guide
- [ ] Profiling benchmarks published

## Milestone 12: Platform Expansion 📅

- [ ] macOS server (I/O Kit USB enumeration)
- [ ] macOS client (IOUSBFamily / uinput alternative)
- [ ] Docker container for server deployment
- [ ] Web-based management UI
- [ ] REST API for server status/control

## Milestone 13: Advanced Features 💡

- [ ] Hot-plug support (detect device attach/detach after server start)
- [ ] Multiple simultaneous client connections to different devices
- [ ] USB isochronous transfer support (audio/video devices)
- [ ] Bandwidth throttling per client
- [ ] Session persistence and auto-reconnect
- [ ] End-to-end latency monitoring dashboard
- [ ] Prometheus metrics endpoint (RUST_LOG structured metrics)
- [ ] USB 3.0 SuperSpeed support (up to 5 Gbps)
- [ ] IPv6 support

## Milestone 14: Ecosystem 💡

- [ ] Home Assistant add-on
- [ ] RetroPie / Lakka integration
- [ ] Steam Link / Moonlight companion
- [ ] Web frontend for device management
- [ ] Mobile companion app (iOS?)
- [ ] Community plugin: USB/IP to network bridge for VM hosts

---

## Version History

| Version | Date | Milestones |
|---------|------|------------|
| 0.1.0 | — | Core protocol, encryption, mDNS |
| 0.2.0 | — | Linux server + client, Windows support |
| 0.3.0 | — | Android app (phone + TV) |
| 0.4.0 | — | Documentation complete, CI/CD |
| 0.5.0 | 2026-06 | Generality test rig, E2E CI, G920 debt retired |
| 1.0.0 | TBD | Stable release with all milestones 1-13 |

---

## Contributing

See the [ARCHITECTURE.md](ARCHITECTURE.md) for design details and [BUILDING.md](docs/BUILDING.md) for build instructions.

Key areas for contribution:
- Debugging and fixing URB transfer edge cases
- Adding macOS support
- Performance profiling on Raspberry Pi / low-end devices
- Adding new USB device descriptor fixtures (see `shared/usbip-core/tests/fixtures/`)
- Translation of documentation
