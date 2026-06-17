# AnyPlug -- Setup Guide

Complete setup instructions for the USB/IP passthrough system on Linux, Windows, and Android.

> **Protocol:** USB/IP v1.1.1 (`0x0111`) on TCP [port 3240](api/PORTS.md) (wire port)
> **mDNS service type:** `_usbip._tcp.local.`

---

## Table of Contents

- [Linux Server Setup](#linux-server-setup)
- [Linux Client Setup (VHCI)](#linux-client-setup-vhci)
- [Windows Server Setup](#windows-server-setup)
- [Windows Client Setup](#windows-client-setup)
- [Android Setup](#android-setup)
- [Encryption Setup](#encryption-setup)
- [mDNS Discovery](#mdns-discovery)
- [Verification](#verification)

---

## Linux Server Setup

### Prerequisites

```bash
sudo apt install libusb-1.0-0-dev pkg-config
# Rust: https://rustup.rs/ (if building from source)
```

### Install

```bash
# Pre-built binary
wget https://github.com/stanvx/anyplug/releases/latest/download/usbip-server-x86_64-linux.tar.gz
tar xzf usbip-server-x86_64-linux.tar.gz
sudo cp usbip-server /usr/local/bin/

# Or build from source
cd ~/anyplug && cargo build --release -p usbip-server
sudo cp target/release/usbip-server /usr/local/bin/
```

### Run

```bash
# Export all USB devices
sudo usbip-server

# Export specific devices only (recommended)
sudo usbip-server --allow 046d:c261 --allow 046d:c262

# With encryption
sudo usbip-server --encrypt

# Custom port
sudo usbip-server --port 3241

# List exportable devices without starting server
usbip-server list
```

### udev Rules

In `/etc/udev/rules.d/99-usbip.rules`:
```
SUBSYSTEM=="usb", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c261", MODE="0666"
```
Then `sudo udevadm control --reload-rules && sudo udevadm trigger`.

### systemd Service

In `/etc/systemd/system/usbip-server.service`:
```ini
[Unit]
Description=AnyPlug Server
After=network.target
[Service]
Type=simple
ExecStart=/usr/local/bin/usbip-server --allow 046d:c261 --allow 046d:c262
Restart=on-failure
RestartSec=5
[Install]
WantedBy=multi-user.target
```
Then `sudo systemctl daemon-reload && sudo systemctl enable --now usbip-server`.

---

## Linux Client Setup (VHCI)

### Full passthrough (kernel VHCI)

```bash
sudo modprobe usbip-vudc
echo "usbip-vudc" | sudo tee -a /etc/modules
sudo usbip-client --connect 192.168.1.100:3240 --busid 1-1
lsusb && dmesg | tail -20  # verify
```

### uinput fallback (input devices only)

```bash
sudo modprobe uinput
usbip-client --connect 192.168.1.100:3240
```

### Auto-discovery via mDNS

```bash
usbip-client --discover
```

---

## Windows Server Setup

### Prerequisites

- Windows 10/11 (64-bit)
- Rust toolchain (if building from source)
- NSIS (if building installer -- see [BUILDING.md](BUILDING.md))

### Option A: Installer (recommended)

1. Download `USB-Passthrough-Setup.exe` from the [releases page](https://github.com/stanvx/anyplug/releases) and run it. The installer configures the binary, Windows Service, and firewall rules for port 3240.

### Option B: Manual / Portable

```powershell
Invoke-WebRequest -Uri "https://github.com/stanvx/anyplug/releases/latest/download/usbip-server-x86_64-windows.zip" -OutFile usbip-server.zip
Expand-Archive usbip-server.zip && cd usbip-server
```

### Run

```powershell
.\usbip-server-windows.exe                      # GUI (system tray)
usbip-server-service.exe install && start       # Windows Service
.\usbip-server.exe --allow 046d:c261            # CLI foreground
```

Run as Administrator: `Start-Process .\usbip-server.exe -Verb RunAs`.

---

## Windows Client Setup

```powershell
# Connect and import device
usbip-client.exe --connect 192.168.1.100:3240 --busid 1-1

# List available devices
usbip-client.exe --list 192.168.1.100:3240

# With encryption
usbip-client.exe --connect 192.168.1.100:3240 --busid 1-1 --encrypt
```

### GUI Client

1. Launch `usbip-client-gui.exe`.
2. Click **"Discover Servers"** (mDNS) or enter a manual address.
3. Select a device and click **"Connect"** -- it appears in Device Manager.

Requires **Administrator privileges** (VHCI driver installation).

---

## Android Setup

### Prerequisites

- Android 11+ (API 30+)
- USB OTG cable (server mode)
- USB Host support
- **Root recommended** for VHCI passthrough. Non-root uses uinput (HID only).

### Install

```bash
wget https://github.com/stanvx/anyplug/releases/latest/download/anyplug-app-release.apk
adb install anyplug-app-release.apk

# TV APK
wget https://github.com/stanvx/anyplug/releases/latest/download/anyplug-tv-release.apk
adb install anyplug-tv-release.apk
```

### Run as Server

1. Connect USB device via OTG.
2. Open the app, tap **Server** tab, tap **"Share"** next to the device.
3. A foreground service starts with a persistent notification.

### Run as Client

1. Open the app, tap **Client** tab.
2. Select a discovered server or enter a manual address.
3. Tap **"Connect"** next to the device.

Rooted: VHCI (local USB). Non-rooted: uinput (HID only, no force feedback).

---

## Encryption Setup

Uses AES-256-GCM with X25519 ECDH key exchange.

### Server

```bash
usbip-server --encrypt
# Output shows public key: [INFO] Server public key: a1b2c3d4e5f6...
```

### Client

```bash
usbip-client --connect 192.168.1.100:3240 --encrypt
# Or pin a known server public key:
usbip-client --connect 192.168.1.100:3240 --encrypt --server-public-key a1b2c3d4e5f6...
```

**How it works:** Server generates X25519 keypair on startup. Client connects, receives server's public key. Both derive shared AES-256-GCM key via ECDH. Each message encrypted with random nonce; 8-byte USB/IP header used as AAD. ~3-8% overhead on fast connections ([PERFORMANCE.md](PERFORMANCE.md)).

---

## mDNS Discovery

Server advertises on `_usbip._tcp.local.`.

Linux: `sudo apt install avahi-daemon && sudo systemctl enable --now avahi-daemon` then `usbip-server` (auto-advertises). Windows: built-in mDNS -- no setup.

### Verify

```bash
usbip-client --discover
dig _usbip._tcp.local. SRV
avahi-browse _usbip._tcp
```

Expected output:
```
Discovered USB/IP servers:
  192.168.1.100:3240  -- devices: 046d:c261, 046d:c262 (example device)
  192.168.1.101:3240  -- devices: 046d:c26b
```

---

## Verification

### Server is running

```bash
ps aux | grep usbip-server
sudo netstat -tlnp | grep 3240  # or: ss -tlnp | grep 3240
journalctl -u usbip-server -f   # systemd logs
```

### Client can connect

```bash
usbip-client --list 192.168.1.100:3240
# Expected:
# Exportable USB devices on 192.168.1.100:3240:
#   046d:c261  1-1  USB HID game controller
```

### Device is usable

```bash
lsusb | grep <vendor>
evtest  # should show the device
```

### Log file locations

| Platform | Path |
|----------|------|
| Linux (systemd) | `journalctl -u usbip-server` |
| Linux (manual) | stderr/stdout; set `RUST_LOG=debug` for verbose |
| Windows (service) | `%PROGRAMDATA%\anyplug\logs\` |
| Windows (GUI) | `%APPDATA%\anyplug\logs\` |
| Android | `adb logcat -s AnyPlug` |

---

## Next Steps

- [Troubleshooting Guide](TROUBLESHOOTING.md)
- [Performance Tuning](PERFORMANCE.md)
- [Building from Source](BUILDING.md)
