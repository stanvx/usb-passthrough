# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

USB/IP passthrough bridge — export a physical USB device on one machine, import it on another as if locally attached. Built on the USB/IP kernel protocol (RFC-compliant, see `PROTOCOL.md`). Target platforms: Linux, Windows (GUI + Service), Android (phone + TV). Alpha status.

## Terminology

The project has strict terminology defined in `CONTEXT.md`:
- **Passthrough**: byte-for-byte forwarding, not HID emulation
- **Server**: machine that exports a physical USB device (libusb/WinUSB/Android USB Host)
- **Client**: machine that imports and presents it locally (vhci-hcd/VHCI driver)
- **Device class scope (v1.0)**: HID, mass storage, USB-to-serial, printers, scanners, bulk-only. Isochronous (audio, webcams) is out of scope
- **G920 debt**: any G920-specific code in `shared/usbip-core/` is a bug — the project supports arbitrary HID, not a specific wheel
- **Test rig**: Linux raw-gadget + dummy_hcd/udc E2E harness on a self-hosted CI runner
- **Reliability primitives**: structured errors with correlation IDs, hot-plug detection, auto-reconnect. Session persistence is deferred
- **Service mode**: headless, survives reboots, no UI after setup (Windows Service / Android foreground service / systemd)

## Build & Test Commands

```bash
# Build entire Rust workspace
cargo build --release

# Run all Rust tests
cargo test --release

# Run tests for a single crate
cargo test --release -p usbip-core
cargo test --release -p usbip-server
cargo test --release -p usbip-client

# Build specific crate
cargo build --release -p usbip-server
cargo build --release -p usbip-client
cargo build --release -p windows

# Check compilation without building (fast)
cargo check --workspace

# Android (requires gradlew wrapper — not yet committed; CI uses bare `gradle`)
cd android
gradle assembleDebug            # debug APK
gradle lintDebug                # Android lint
gradle :app:assembleRelease     # phone APK only
gradle :tv:assembleRelease      # TV APK only

# Format and lint
cargo fmt --all -- --check    # check formatting
cargo fmt --all               # auto-format
cargo clippy --workspace -- -D warnings   # strict lint
```

## Conventions

- Prefer immutability — never mutate arguments, return new values
- Many small files (200-400 lines typical, 800 max)
- No emojis in code, comments, or docs
- Conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`
- Test before committing — run `/verify` (see `.claude/skills/verify/`)

## Architecture

### Workspace Structure (4 Rust crates)

```
Cargo.toml (workspace root)
├── shared/usbip-core/    # Protocol types, crypto, errors — no platform deps
├── server/usbip-server/  # USB/IP server binary (libusb, mDNS, tokio async TCP)
├── client/usbip-client/  # USB/IP client binary (VHCI driver, mDNS, tokio)
└── windows/              # Windows egui app + Windows Service + SetupAPI enum
```

### Data Flow

```
export machine (Server)                    import machine (Client)
Physical USB ← libusb/WinUSB ← TCP:3240 → VHCI driver → OS USB stack → App
```

A single URB round-trip: kernel ioctl → TCP send → network → TCP recv → USB controller → response. ~700µs RTT on gigabit Ethernet.

### Key Dependencies

- `zerocopy 0.8` — zero-copy wire-format serialization in protocol.rs/urb.rs
- `crc32fast 1.4` — CRC32 for USB/IP headers
- Windows crate: `egui`/`eframe`/`tray-icon` for GUI + system tray, `crossbeam-channel` for IPC, `winres` for resource embedding
- Server and client crates expose a `lib.rs` public API — the windows crate depends on both as libraries, not just binaries

### Thread Model

Server: main → mDNS thread + TCP accept loop → per-client task (handle_devlist / handle_import / urb_loop) + libusb hotplug monitor.
Client: main → mDNS browser + TCP connect → URB send/receive threads + VHCI dispatch + VHCI event thread for kernel completions.

### Security

AES-256-GCM tunnel (optional, per `--encrypt` flag). X25519 ECDH key exchange with pure-Rust field arithmetic. Pre-shared key or QR code pairing. Device VID:PID allowlisting. Connection confirmation prompt. See `SECURITY.md`.

## Known Gaps

- **Android build**: `gradlew` wrapper and `android/rust/usbip-android/` JNI crate are not yet
  committed — `gradle assembleDebug` and CI Android lint won't work from a fresh checkout.
- **Test coverage**: only `crypto.rs` and `windows_usb.rs` have tests; server, client, protocol,
  and URB modules have none. `cargo test -p usbip-server` and `-p usbip-client` pass vacuously.
- **Release workflow**: Windows and Android build steps use `continue-on-error: true`, so a
  failed build still produces an empty release artifact.

## CI

GitHub Actions (`.github/workflows/ci.yml`):
- `cargo check --workspace` + `cargo test --workspace` on ubuntu-latest (Rust stable)
- `./gradlew lintDebug` on ubuntu-latest with JDK 17 (Temurin)

Release workflow (`.github/workflows/release.yml`) handles multi-platform binary builds.

## Documentation Index

| File | Purpose |
|------|---------|
| `ARCHITECTURE.md` | Full system design, thread model, data flow, platform specifics, dependency map |
| `PROTOCOL.md` | USB/IP wire protocol reference |
| `ROADMAP.md` | Milestone tracking (M1-M14, past and future) |
| `CONTEXT.md` | Canonical terminology and naming conventions |
| `CONTRIBUTING.md` | Contribution guidelines |
| `SECURITY.md` | Security model and vulnerability reporting |
| `docs/BUILDING.md` | Build from source for all platforms |
| `docs/SETUP.md` | Platform-specific setup guides |
| `docs/PERFORMANCE.md` | Latency budget, tuning, benchmarks |
| `docs/TROUBLESHOOTING.md` | Common issues and fixes |
| `docs/G920-SPECIFIC.md` | Reference device quirks |
| `docs/ANDROID-TV.md` | TV-specific setup, sideloading, remote nav |
| `docs/adr/` | Architecture Decision Records (3 ADRs) |

## Agent skills

### Issue tracker

GitHub Issues, via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Standard labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.
