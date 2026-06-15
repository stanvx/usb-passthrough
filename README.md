# AnyPlug — Cross-Platform USB/IP Bridge

**Pass any USB device over the network between Android, Android TV, and Windows with sub-millisecond latency.**

Cross-platform USB/IP bridge built on the Linux kernel USB/IP protocol (see [PROTOCOL.md](PROTOCOL.md)).

### Use Case

```
Phone (Server, USB device attached) ◄── Wi-Fi ──► TV (Client, sees remote USB)
```

All device features — native drivers, FFB, gamepad rumble — work as if locally connected.

---

## Supported Platforms

| Platform         | Server (export) | Client (import) | Service Mode |
|------------------|:---------------:|:---------------:|:------------:|
| **Windows 10/11**| ✅              | ✅              | ✅ Windows Service |
| **Android 9+**   | ✅ USB Host     | ✅ VHCI module  | ✅ Foreground Service |
| **Android TV 9+**| ⚠️ Limited      | ✅              | ✅ Foreground Service |
| **Linux**        | ✅ (usbip-host) | ✅ (vhci-hcd)   | ✅ systemd |

---

## Quick Start

### Windows → Windows

```powershell
winget install USB-Passthrough
anyplug serve --device "My Keyboard"    # machine with USB device
anyplug connect --server 192.168.1.100 --device "My Keyboard"  # gaming machine
```

### Android Phone → Android TV

Install APKs on both devices. Plug USB device into phone via USB-C hub.
Open the app on both — auto-discovery via mDNS. Tap the device on TV to connect.

---

## Key Features

- **True USB passthrough** — not HID emulation. FFB, gamepad rumble, pedals all work.
- **USB/IP protocol** — same as Linux kernel, battle-tested since 2008.
- **mDNS discovery** — no IP config needed, devices find each other.
- **AES-256-GCM encryption** — optional, for untrusted networks.
- **Sub-1ms per-URB latency** on Ethernet, 2-5ms on Wi-Fi 6.
- **Service mode** — runs headless, survives reboots.
- **Android TV UI** — D-pad navigable, remote-friendly.
- **Auto-reconnect** — survives network flaps and device cycles.

---

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md).

```
┌────────────────────────────────────────────────┐
│  Android · Android TV · Windows (frontends)    │
│  ┌──────────────────────────────────────────┐  │
│  │  Rust USB/IP Core (Protocol · URB · mDNS)│  │
│  └────────────────┬─────────────────────────┘  │
│  ┌────────────────┴─────────────────────────┐  │
│  │  Platform USB Stack (libusb/WinUSB/Android)│ │
│  └──────────────────────────────────────────┘  │
└────────────────────────────────────────────────┘
```

---

## Latency Budget

HID URB round-trip on gigabit Ethernet: **~700 µs total RTT**.

For FFB at 250 Hz (<4ms needed): 5x headroom on Ethernet, 2x on good Wi-Fi.

---

## Building From Source

Prerequisites: Rust 1.78+, Android SDK 34+, JDK 17+

```bash
git clone https://github.com/stanvx/anyplug && cd anyplug
cargo build --release                       # all Rust crates
cd android && ./gradlew assembleRelease     # Android APK
cd windows/installer && makensis installer.nsi  # Windows installer
```

See [docs/BUILDING.md](docs/BUILDING.md) for platform-specific details.

---

## Documentation Index

| Document | What's Inside |
|----------|---------------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Full system design, data flow, thread model |
| [PROTOCOL.md](PROTOCOL.md) | USB/IP wire protocol reference |
| [docs/SETUP.md](docs/SETUP.md) | Step-by-step setup per platform |
| [docs/ANDROID-TV.md](docs/ANDROID-TV.md) | TV-specific setup, sideloading, remote navigation |
| [docs/PERFORMANCE.md](docs/PERFORMANCE.md) | Tuning, buffer sizes, Wi-Fi vs Ethernet |
| [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) | Common issues and fixes |
| [docs/BUILDING.md](docs/BUILDING.md) | Compile from source |

---

## License

MIT — see [LICENSE](LICENSE).

## Status

**Alpha.** Core protocol works. Android TV client needs VHCI kernel module on rooted devices. See [ROADMAP.md](ROADMAP.md) for what's coming.
