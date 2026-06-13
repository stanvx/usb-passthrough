# USB/IP Passthrough — Setup Guide

Complete setup instructions for the USB/IP passthrough system on Linux, Windows, and Android.

> **Project:** USB/IP Passthrough for Logitech G920 (and other USB devices)  
> **Protocol:** USB/IP v1.1.1 (`0x0111`) on TCP port 3240  
> **Default port:** 3240  
> **mDNS service type:** `_usbip._tcp.local.`

---

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Linux Server Setup](#linux-server-setup)
- [Linux Client Setup (VHCI)](#linux-client-setup-vhci)
- [Windows Server Setup](#windows-server-setup)
- [Windows Client Setup](#windows-client-setup)
- [Android Setup](#android-setup)
- [Encryption Setup](#encryption-setup)
- [mDNS Discovery](#mdns-discovery)
- [Verification](#verification)

---

## Architecture Overview

The system has two roles:

```
┌──────────────┐   USB/IP (TCP :3240)   ┌──────────────┐
│  USB/IP      │ ◄────────────────────► │  USB/IP      │
│  Server      │      may encrypt       │  Client      │
│ (has device) │    (AES-256-GCM)       │ (wants dev)  │
└──────────────┘                        └──────┬───────┘
       ▲                                       │
       │ USB                                   │ VHCI / uinput
       ▼                                       ▼
  ┌──────────┐                          ┌──────────────┐
  │ G920     │                          │ /dev/usbip*  │
  │ Wheel    │                          │ or /dev/uinput│
  └──────────┘                          └──────────────┘
```

- **Server** — has the physical USB device attached. Exports it over TCP.
- **Client** — connects to the server, imports the device, presents it via VHCI or uinput.

Both roles can run on any platform. A Linux server can serve a Windows client, and vice versa.

---

## Linux Server Setup

### Prerequisites

```bash
# libusb 1.0 (for USB device enumeration/access)
sudo apt install libusb-1.0-0-dev pkg-config

# Rust toolchain (if building from source)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Install the Server

#### Pre-built binary

```bash
# Download the latest release
wget https://github.com/stanvx/usb-passthrough/releases/latest/download/usbip-server-x86_64-linux.tar.gz
tar xzf usbip-server-x86_64-linux.tar.gz
sudo cp usbip-server /usr/local/bin/
```

#### Build from source

```bash
cd /home/localadmin/usb-passthrough
cargo build --release -p usbip-server
sudo cp target/release/usbip-server /usr/local/bin/
```

### Run the Server

**Basic — export all USB devices:**

```bash
sudo usbip-server
```

**Export only specific devices (recommended for security):**

```bash
sudo usbip-server --allow 046d:c261 --allow 046d:c262
```

**With encryption:**

```bash
sudo usbip-server --encrypt
```

**Custom port:**

```bash
sudo usbip-server --port 3241
```

**List exportable devices without starting server:**

```bash
usbip-server list
```

### udev Rules (permanent device access)

Create `/etc/udev/rules.d/99-usbip.rules`:

```
# Grant non-root access to USB devices for usbip-server
SUBSYSTEM=="usb", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c261", MODE="0666"
SUBSYSTEM=="usb", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c262", MODE="0666"

# Generic rule for any device you want to export
SUBSYSTEM=="usb", ATTRS{idVendor}=="VVVV", ATTRS{idProduct}=="PPPP", MODE="0666"
```

Then reload:

```bash
sudo udevadm control --reload-rules
sudo udevadm trigger
```

### systemd Service

Create `/etc/systemd/system/usbip-server.service`:

```ini
[Unit]
Description=USB/IP Passthrough Server
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/usbip-server --allow 046d:c261 --allow 046d:c262
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now usbip-server
```

---

## Linux Client Setup (VHCI)

### Option A: Kernel VHCI (full USB device passthrough)

Requires the Linux VHCI kernel module.

```bash
# Load the VHCI module
sudo modprobe usbip-vudc

# Make it persistent
echo "usbip-vudc" | sudo tee -a /etc/modules

# Start the usbip client
sudo usbip-client --connect 192.168.1.100:3240 --busid 1-1
```

The device will appear as a locally-attached USB device. Check with:

```bash
lsusb
dmesg | tail -20
```

### Option B: uinput fallback (Input devices only)

If VHCI is unavailable, the client uses `/dev/uinput` to create input devices:

```bash
# Ensure uinput is available
sudo modprobe uinput

# Run client (auto-detects if VHCI is available)
usbip-client --connect 192.168.1.100:3240
```

### Auto-discovery via mDNS

```bash
# Browse for servers on the local network
usbip-client --discover
```

---

## Windows Server Setup

### Prerequisites

- Windows 10/11 (64-bit)
- [Rust toolchain](https://rustup.rs/) (if building from source)
- NSIS (if building installer — see [BUILDING.md](BUILDING.md))

### Option A: Installer (recommended)

1. Download the latest USB Passthrough installer from the [releases page](https://github.com/stanvx/usb-passthrough/releases).
2. Run `USB-Passthrough-Setup.exe`.
3. Follow the wizard. The installer will:
   - Install the service binary
   - Register the Windows Service (`usbip-server-service`)
   - Add firewall rules for port 3240

### Option B: Manual / Portable

```powershell
# Download the portable archive
Invoke-WebRequest -Uri "https://github.com/stanvx/usb-passthrough/releases/latest/download/usbip-server-x86_64-windows.zip" -OutFile usbip-server.zip
Expand-Archive usbip-server.zip
cd usbip-server
```

### Run the Server

**As a GUI app (egui tray):**

```powershell
.\usbip-server-windows.exe
```

This opens a system tray icon. Right-click to:
- Start/stop the server
- View connected clients
- Enable encryption
- Open logs

**As a Windows Service:**

```powershell
# Install the service
usbip-server-service.exe install

# Start the service
usbip-server-service.exe start

# Verify
Get-Service usbip-server-service
```

**Command-line (foreground):**

```powershell
.\usbip-server.exe --allow 046d:c261
```

### UAC and Driver Permissions

On Windows, USB device access requires administrator privileges for raw device enumeration:

```powershell
# Run the server as administrator
Start-Process .\usbip-server.exe -Verb RunAs
```

---

## Windows Client Setup

### Prerequisites

- Windows 10/11 (64-bit)
- **Administrator privileges** (required for VHCI driver installation)

### Using the GUI Client

1. Launch the USB Passthrough client app (`usbip-client-gui.exe`).
2. Click **"Discover Servers"** to find servers via mDNS (local network only).
3. Or enter a server address manually: `192.168.1.100:3240`.
4. Select a device from the list and click **"Connect"**.
5. The device will appear in Device Manager as a new USB device.

### Command-line Client

```powershell
# Connect to a server and import a device
usbip-client.exe --connect 192.168.1.100:3240 --busid 1-1

# List available devices on a server
usbip-client.exe --list 192.168.1.100:3240

# With encryption
usbip-client.exe --connect 192.168.1.100:3240 --busid 1-1 --encrypt
```

---

## Android Setup

### Prerequisites

- Android 11+ (API 30+) recommended
- USB OTG cable (for server mode)
- Android device with USB Host support
- **Root recommended** — VHCI passthrough requires kernel support. Non-root fallback uses uinput (limited to HID).

### Install from APK

```bash
# Download the APK from releases
wget https://github.com/stanvx/usb-passthrough/releases/latest/download/usb-passthrough-app-release.apk

# Install via adb
adb install usb-passthrough-app-release.apk
```

Or download the Android TV APK:

```bash
wget https://github.com/stanvx/usb-passthrough/releases/latest/download/usb-passthrough-tv-release.apk
adb install usb-passthrough-tv-release.apk
```

### Run as Server

1. Connect the G920 to your Android device via USB OTG.
2. Open the "USB Passthrough" app.
3. Tap the **Server** tab.
4. You'll see "Local USB Devices" — tap **"Share"** next to your wheel.
5. The app starts a foreground service with a persistent notification.

The server is now broadcasting via mDNS and listening on TCP 3240.

### Run as Client

1. Open the app.
2. Tap the **Client** tab.
3. Select a discovered server, or enter a manual address.
4. Tap **"Connect"** next to the device you want to import.

**Rooted device:** The device attaches via VHCI and appears as local USB.

**Non-rooted device:** Only HID devices are supported (via `/dev/uinput`). The wheel's force feedback may be limited.

---

## Encryption Setup

The project supports AES-256-GCM encryption with X25519 ECDH key exchange.

### Server-side

```bash
# Enable encryption (auto-generates key pair on first run)
usbip-server --encrypt

# Output will show the public key:
# [INFO] Server public key: a1b2c3d4e5f6...
```

### Client-side

```bash
# Connect with encryption
usbip-client --connect 192.168.1.100:3240 --encrypt

# Or specify a known public key for pinned verification
usbip-client --connect 192.168.1.100:3240 --encrypt --server-public-key a1b2c3d4e5f6...
```

### How It Works

1. Server generates an X25519 key pair on startup.
2. Client connects, receives the server's public key.
3. Both sides derive a shared AES-256-GCM key via ECDH.
4. All subsequent USB/IP messages are encrypted with a random nonce per message.
5. The 8-byte USB/IP header is used as AAD (Additional Authenticated Data).

**Performance impact:** ~3-8% overhead on fast connections. See [PERFORMANCE.md](PERFORMANCE.md).

---

## mDNS Discovery

The server advertises itself via mDNS (`_usbip._tcp.local.`).

### Enable on Linux

```bash
# Avahi is the mDNS responder on most Linux distros
sudo apt install avahi-daemon
sudo systemctl enable --now avahi-daemon

# Our server automatically starts mDNS advertisement
usbip-server
```

### Enable on Windows

mDNS works out of the box on Windows 10/11 via the built-in mDNS resolver. No additional setup needed.

### Verify Discovery

```bash
# From a client machine
usbip-client --discover

# Or use dig
dig _usbip._tcp.local. SRV

# Or use avahi-browse
avahi-browse _usbip._tcp
```

Expected output:

```
Discovered USB/IP servers:
  192.168.1.100:3240  — devices: 046d:c261 (G920), 046d:c262 (G920)
  192.168.1.101:3240  — devices: 046d:c26b (G29)
```

---

## Verification

### Server is running

```bash
# Check the server process
ps aux | grep usbip-server

# Verify port is listening
sudo netstat -tlnp | grep 3240
# Or: ss -tlnp | grep 3240

# Check logs (if using systemd)
journalctl -u usbip-server -f
```

### Client can connect

```bash
# List devices on a server
usbip-client --list 192.168.1.100:3240

# Expected output:
# Exportable USB devices on 192.168.1.100:3240:
#   046d:c261  1-1  Logitech G920 Racing Wheel
```

### Device is usable

```bash
# On Linux client — the device should appear in lsusb
lsusb | grep Logitech

# Check input devices
evtest  # should show the wheel
```

### Log file locations

| Platform | Path |
|----------|------|
| Linux (systemd) | `journalctl -u usbip-server` |
| Linux (manual) | stderr/stdout; set `RUST_LOG=debug` for verbose |
| Windows (service) | `%PROGRAMDATA%\usb-passthrough\logs\` |
| Windows (GUI) | `%APPDATA%\usb-passthrough\logs\` |
| Android | Logcat: `adb logcat -s UsbPassthrough` |

---

## Next Steps

- [Troubleshooting Guide](TROUBLESHOOTING.md)
- [G920-Specific Configuration](G920-SPECIFIC.md)
- [Performance Tuning](PERFORMANCE.md)
- [Building from Source](BUILDING.md)
