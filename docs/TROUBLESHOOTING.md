# USB/IP Passthrough — Troubleshooting Guide

Diagnostic procedures and solutions for common USB/IP passthrough issues.

> If you're here because something isn't working: start at **[Quick Diagnostic Flow](#quick-diagnostic-flow)**, which will guide you step-by-step.

---

## Table of Contents

- [Quick Diagnostic Flow](#quick-diagnostic-flow)
- [Connection Issues](#connection-issues)
- [Device Not Found](#device-not-found)
- [Permission Errors](#permission-errors)
- [VHCI Driver Problems](#vhci-driver-problems)
- [Android Issues](#android-issues)
- [Windows Issues](#windows-issues)
- [mDNS Discovery Issues](#mdns-discovery-issues)
- [Encryption Issues](#encryption-issues)
- [Logs and Debugging](#logs-and-debugging)

---

## Quick Diagnostic Flow

```
Device not working?
│
├─ Can server see the device?
│   $ usbip-server list
│   └─ No  → Check USB connection, permissions
│
├─ Can client reach server?
│   $ usbip-client --list <SERVER_IP>:3240
│   └─ No  → Firewall? Server running? Network?
│
├─ Can client import the device?
│   $ usbip-client --connect <SERVER_IP>:3240 --busid <BUSID>
│   └─ No  → Check VHCI driver (Linux: modprobe usbip-vudc)
│             Check Windows: driver signature? Admin?
│             Check Android: rooted? Wake lock?
│
├─ Does device appear locally?
│   $ lsusb (Linux) | Device Manager (Win) | Settings (Android)
│   └─ Yes → Test with application
│   └─ No  → VHCI not installed / imported but not attached
│
└─ Device appears but doesn't work?
    Check: speed match? URB timeouts? USB descriptor issues?
    See G920-SPECIFIC.md for wheel-specific fixes.
```

---

## Connection Issues

### "Connection refused" — server not reachable

**Symptoms:**
```
usbip-client --connect 192.168.1.100:3240
Error: connection refused (OS error 111)
```

**Diagnosis:**

```bash
# Is the server running?
ssh user@server "ps aux | grep usbip-server"

# Is the port open?
nc -zv 192.168.1.100 3240

# Check firewall on server
sudo iptables -L -n | grep 3240
# Or: sudo nft list ruleset | grep 3240
```

**Fixes:**

1. Start the server: `sudo systemctl start usbip-server`
2. Open the firewall:
   ```bash
   sudo ufw allow 3240/tcp
   # Or with iptables:
   sudo iptables -A INPUT -p tcp --dport 3240 -j ACCEPT
   ```
3. Verify bind address: if server is bound to `127.0.0.1`, it won't accept external connections. Use `--bind 0.0.0.0` (default) or a specific interface IP.

### "Connection timed out"

**Symptoms:**
```
usbip-client --connect 192.168.1.100:3240
Error: connection timed out
```

**Diagnosis:**
```bash
# Check network path
traceroute 192.168.1.100

# Check if port is filtered (vs. closed)
# Filtered = firewall drops silently, Closed = rejects with RST
nmap -p 3240 192.168.1.100
```

**Fixes:**
- Ensure no intermediate firewall blocks outbound 3240.
- If on different subnets, ensure routing allows traffic.
- Wi-Fi: some routers isolate clients. Try wired Ethernet.

### "Connection reset by peer"

**Symptoms:**
```
Error: connection reset by peer (OS error 104)
```

**Causes and fixes:**

| Cause | Fix |
|-------|-----|
| Server rejected import (device already exported) | Use a different device, or disconnect other clients |
| Encryption mismatch (one side has encryption, other doesn't) | Both sides must use `--encrypt` or both not |
| Protocol version mismatch | Ensure both sides are same version (`0x0111`) |
| Server crashed mid-handshake | Restart server; check logs |

---

## Device Not Found

### Server shows no devices

```bash
$ usbip-server list
Exportable USB devices:
(empty)
```

**Diagnosis:**

```bash
# Is the device physically connected?
lsusb | grep Logitech
# Should show: Bus 001 Device 003: ID 046d:c261 Logitech G920 Racing Wheel

# Does the user have permission?
ls -la /dev/bus/usb/001/003
# Should show: crw-rw-r--   ... If not, set udev rule

# Is libusb working?
lsusb -v 2>&1 | grep -i "couldn't"
# If you see "couldn't open device" — permission issue
```

**Fixes:**

1. Plug device into a different USB port (try USB 2.0 port for G920).
2. Apply udev rules (see [SETUP.md](SETUP.md#udev-rules-permanent-device-access)).
3. Run server as root: `sudo usbip-server`.
4. On Windows: run as Administrator.

### Client can list server, but import fails

**Symptoms:**
```
usbip-client --connect 192.168.1.100:3240 --busid 1-1
Error: import rejected (status: 2 — device busy)
```

**Fixes:**
- Device is already exported to another client. Wait or disconnect other clients.
- On Linux server: another process (like `usbip-host` from the kernel module) may be using the device.
- Check server logs: `journalctl -u usbip-server -n 50`

---

## Permission Errors

### Linux — "Cannot open device"

```
Error: NotSupported("libusb: couldn't open USB device: Permission denied")
```

**Fix:** Install udev rules:

```bash
# Create udev rule for G920
echo 'SUBSYSTEM=="usb", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c261", MODE="0666"' | sudo tee /etc/udev/rules.d/99-g920.rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

### Linux — VHCI permission denied

```
Error: failed to open /dev/usbip-vudc: Permission denied
```

**Fix:** Add your user to the appropriate group or run as root:

```bash
# Check device permissions
ls -la /dev/usbip-vudc

# Run client with sudo
sudo usbip-client --connect 192.168.1.100:3240 --busid 1-1
```

### Windows — "Access Denied" on USB device

**Fix:** Run the application as Administrator:
- Right-click → "Run as Administrator"
- Or launch from an elevated PowerShell: `Start-Process .\usbip-server.exe -Verb RunAs`

### Windows — Service won't start

```
Error 5: Access Denied
```

- The service runs as `LocalSystem` by default. If it needs user-specific device access, change the service account:
  ```powershell
  sc.exe config usbip-server-service obj=".\Administrator" password="***"
  ```

---

## VHCI Driver Problems

### Linux — VHCI module not found

```
$ sudo modprobe usbip-vudc
modprobe: FATAL: Module usbip-vudc not found in directory /lib/modules/...
```

**Fixes:**

1. Install kernel headers and build the module:
   ```bash
   sudo apt install linux-headers-$(uname -r)
   # The module may need to be built separately:
   cd /usr/src/linux-headers-$(uname -r)
   make modules_prepare
   ```

2. Use uinput fallback instead:
   ```bash
   sudo modprobe uinput
   usbip-client --connect 192.168.1.100:3240 --busid 1-1 --use-uinput
   ```

3. This kernel doesn't have VHCI support enabled. Consider a custom kernel or different distro.

### Linux — VHCI import succeeds but no device appears

```
Import successful, device busid: 1-1
```

But `lsusb` shows nothing.

**Check:**

```bash
# Is the VHCI driver loaded?
lsmod | grep usbip

# Check dmesg for errors
dmesg | grep usbip

# Did the client attach the device?
usbip-client status
```

**Fix:** The client may not have called the attach ioctl. Ensure the client runs with appropriate permissions.

### Windows — VHCI driver not installed

The Windows VHCI is installed automatically by the installer. If missing:

```powershell
# Check if the driver is present
pnputil /enum-drivers | findstr usbip

# Install manually from admin prompt
pnputil /add-driver "C:\Program Files\USB Passthrough\drivers\usbipvhci.inf" /install
```

---

## Android Issues

### Device not detected on Android

**Check:**
1. Does the device appear in `lsusb` via ADB?
   ```bash
   adb shell lsusb
   ```
2. Does the app show "Local USB Devices"?
   - No → USB OTG not connected or not supported.
3. Try a different USB cable (OTG cable required).
4. Check Android USB settings: "USB controlled by" → "This device"

### Foreground service stops

Android may kill the foreground service if the device is under memory pressure.

**Fix:**
- Disable battery optimization for the app:
  ```
  Settings → Apps → USB Passthrough → Battery → Unrestricted
  ```
- In the app, the persistent notification prevents most Android versions from killing it.

### "USB device not accessible" on Android 12+

Android 12+ restricts USB device access. Grant permission:

1. When the app asks for USB permission, tap "Allow".
2. If missed, go to: `Settings → Connected devices → USB → USB Passthrough → Grant`.

### Non-rooted device — uinput fallback issues

Without root, VHCI is unavailable. The app falls back to uinput for HID devices:

```bash
# Check if uinput node exists
adb shell ls -la /dev/uinput
# If missing: kernel doesn't have CONFIG_INPUT_UINPUT
```

**Limitations on non-rooted:**
- Only HID devices supported.
- Force feedback will NOT work (requires kernel-level access).
- Some complex HID devices may not function fully.

---

## Windows Issues

### Device not enumerating on server

The Windows server uses Win32 SetupAPI to enumerate USB devices.

**Check:**
```powershell
# Run the server with verbose logging
$env:RUST_LOG="debug"
.\usbip-server.exe

# Look for lines like:
# DEBUG usbip_server::winusb: Found device: 046d:c261 - Logitech G920
```

If devices aren't found:
1. Run as Administrator (many USB APIs require elevation).
2. Check Device Manager: is the wheel visible in "Human Interface Devices"?
3. Try a different USB port.

### Firewall blocking connection

Windows Defender Firewall may block usbip-server.

```powershell
# Add rule for the server
New-NetFirewallRule -DisplayName "USB Passthrough Server" `
  -Direction Inbound -Protocol TCP -LocalPort 3240 -Action Allow

# Add rule for the client
New-NetFirewallRule -DisplayName "USB Passthrough Client" `
  -Direction Outbound -Protocol TCP -RemotePort 3240 -Action Allow
```

### GUI tray icon not appearing

- Check notification area settings: the icon may be hidden.
- Click the arrow in the system tray to show hidden icons.
- Drag the icon to the taskbar if desired.

---

## mDNS Discovery Issues

### Server not discoverable

**From server:**
```bash
# Check if mDNS is registering
RUST_LOG=debug usbip-server 2>&1 | grep mdns
# Expected: "mDNS advertised: myhost._usbip._tcp.local. on 192.168.1.100:3240"
```

**From client:**
```bash
# Check if mDNS browse finds anything
RUST_LOG=debug usbip-client --discover 2>&1 | grep mdns

# Test mDNS more broadly
avahi-browse -a
# Windows: dns-sd -B _usbip._tcp local
```

**Fixes:**
1. On Linux: ensure Avahi is running: `sudo systemctl status avahi-daemon`
2. mDNS is link-local only — both machines must be on the same subnet.
3. Some enterprise Wi-Fi networks block mDNS multicast. Use direct IP connection instead.
4. Windows Firewall can block mDNS (UDP 5353). Ensure it's allowed:
   ```powershell
   New-NetFirewallRule -DisplayName "mDNS" -Direction Inbound -Protocol UDP -LocalPort 5353 -Action Allow
   ```

### Slow device listing via mDNS

mDNS responses depend on network conditions and the number of responders.

- The client waits 2 seconds by default for mDNS responses.
- If servers respond slowly, increase the timeout in the source (`discovery.rs`, line 35: `Duration::from_secs(2)`).

---

## Encryption Issues

### "Crypto handshake failed"

**Symptoms:**
```
Error: CryptoError: handshake failed
```

**Fixes:**
- Both sides must use `--encrypt` flag. Mixed encrypted/unencrypted connections are rejected.
- Check that `ring` crate dependencies are available. On some platforms, `ring` requires specific build tools:
  ```bash
  # macOS
  # On Linux: clang, libclang-dev
  sudo apt install clang libclang-dev
  ```

### "Decryption failed / authentication tag mismatch"

**Symptoms:**
```
Error: CryptoError: DecryptError
```

**Fixes:**
- Data corruption in transit. Test with `tcp_nodelay=true` (default).
- Key mismatch: the client and server must derive the same session key.
- If the connection goes through a proxy or load balancer that modifies packets, encryption will fail.

### Performance degradation with encryption

AES-256-GCM adds ~12 bytes per message (nonce) plus ~16 bytes authentication tag. For the G920's high URB rate, this adds to CPU usage.

- On underpowered Android devices, encryption may cause noticeable latency.
- Consider Ethernet rather than Wi-Fi when using encryption.
- See [PERFORMANCE.md](PERFORMANCE.md#encryption-overhead) for benchmarks.

---

## Logs and Debugging

### Enable verbose logging

**All platforms** — set the `RUST_LOG` environment variable:

```bash
# Levels: error, warn, info, debug, trace
RUST_LOG=debug usbip-server
RUST_LOG=trace usbip-server  # Very verbose — URB dumps
```

### Log file locations

| Platform | Location |
|----------|----------|
| Linux (systemd) | `journalctl -u usbip-server -f` |
| Linux (manual) | stderr/stdout |
| Windows (service) | `%PROGRAMDATA%\usb-passthrough\logs\service.log` |
| Windows (GUI) | `%APPDATA%\usb-passthrough\logs\app.log` |
| Android | `adb logcat -s UsbPassthrough,RustBridge` |

### Common log patterns

```
# Device exported successfully
INFO usbip_server: Device 046d:c261 exported on bus 1-1

# Client connected
INFO usbip_server::server: Client connected: 192.168.1.50:54321

# URB transfer
TRACE usbip_server::usb: URB seqnum=42 devid=1 ep=0x81 dir=IN len=64

# Client disconnected
INFO usbip_server::server: Client disconnected: 192.168.1.50:54321

# Error: device busy
WARN usbip_server::server: Import rejected — device 046d:c261 already exported
```

### Analyzing URB traffic

With `RUST_LOG=trace`, each URB is logged. Example:

```
TRACE usbip_server::usb: URB seqnum=1024 ep=0x01 dir=OUT flags=0x0000 len=8 data=[...]
TRACE usbip_server::usb: URB seqnum=1025 ep=0x81 dir=IN flags=0x0200 actual=64 data=[...]
```

Use this to verify the G920's control transfers (setup packets) and endpoint 0x81 (interrupt IN) traffic.

---

## Known Issues

| Issue | Status | Workaround |
|-------|--------|------------|
| G920 force feedback not working on non-rooted Android | By design (no kernel VHCI) | Use rooted device or Linux client |
| Windows VHCI driver requires reboot after install | Known limitation | Reboot once after first install |
| mDNS not working across VLANs | mDNS protocol limit | Use direct IP: `usbip-client --connect <IP>:3240` |
| Encryption adds latency on Raspberry Pi | Expected (no HW AES) | Disable encryption on LAN |
| Some USB 3.0 ports give G920 enumeration issues | Known G920 hardware quirk | Use USB 2.0 port |
