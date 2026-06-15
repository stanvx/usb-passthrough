# AnyPlug -- Troubleshooting Guide

Diagnostic procedures and solutions for common USB/IP passthrough issues.

> Start at **[Quick Diagnostic Flow](#quick-diagnostic-flow)** if something isn't working.

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

  Can server see the device?
  $ usbip-server list
  No  -> Check USB connection, permissions

  Can client reach server?
  $ usbip-client --list <SERVER_IP>:3240
  No  -> Firewall? Server running? Network?

  Can client import the device?
  $ usbip-client --connect <SERVER_IP>:3240 --busid <BUSID>
  No  -> Check VHCI driver (modprobe usbip-vudc),
          Windows driver signature / admin,
          Android root / wake lock

  Does device appear locally?
  $ lsusb (Linux) | Device Manager (Win) | Settings (Android)
  No  -> VHCI not installed / import succeeded but not attached

  Device appears but doesn't work?
  Check: speed match, URB timeouts, USB descriptor issues
  -- see platform sections below
```

---

## Connection Issues

### "Connection refused"

```
Error: connection refused (OS error 111)
```

```bash
ssh user@server "ps aux | grep usbip-server"
nc -zv 192.168.1.100 3240
sudo iptables -L -n | grep 3240
```

**Fixes:** Start server (`sudo systemctl start usbip-server`). Open firewall (`sudo ufw allow 3240/tcp`). Verify server isn't bound to `127.0.0.1`.

### "Connection timed out"

```bash
traceroute 192.168.1.100
nmap -p 3240 192.168.1.100  # Filtered vs closed
```

**Fixes:** Check intermediate firewalls, routing between subnets, Wi-Fi client isolation. Try wired Ethernet.

### "Connection reset by peer"

| Cause | Fix |
|-------|-----|
| Device already exported | Use different device or disconnect other clients |
| Encryption mismatch | Both sides must use `--encrypt` or neither |
| Protocol version mismatch | Ensure same binary version (`0x0111`) |
| Server crashed mid-handshake | Restart server; check logs |

---

## Device Not Found

### Server shows no devices

```
$ usbip-server list
Exportable USB devices: (empty)
```

```bash
lsusb | grep <vendor>
ls -la /dev/bus/usb/001/003   # check permissions
```

**Fixes:** Try different USB port (USB 2.0 for some devices). Apply udev rules ([SETUP.md](SETUP.md)). Run server as root/sudo. Windows: run as Administrator.

### Client can list server, but import fails

```
Error: import rejected (status: 2 -- device busy)
```

**Fixes:** Another client has the device. Wait or disconnect. On Linux: check if `usbip-host` kernel module is using it. Check server logs: `journalctl -u usbip-server -n 50`.

---

## Permission Errors

### Linux -- "Cannot open device"

```
Error: NotSupported("libusb: couldn't open USB device: Permission denied")
```

**Fix:** `echo 'SUBSYSTEM=="usb", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c261", MODE="0666"' | sudo tee /etc/udev/rules.d/99-usb-device.rules && sudo udevadm control --reload-rules && sudo udevadm trigger`

### Linux -- VHCI permission denied

```
Error: failed to open /dev/usbip-vudc: Permission denied
```

**Fix:** `sudo usbip-client --connect 192.168.1.100:3240 --busid 1-1`

### Windows -- "Access Denied"

**Fix:** Right-click "Run as Administrator" or `Start-Process .\usbip-server.exe -Verb RunAs`.

### Windows -- Service won't start (Error 5)

```
Error 5: Access Denied
```

**Fix:** `sc.exe config usbip-server-service obj=".\Administrator" password="***"`

---

## VHCI Driver Problems

### Linux -- Module not found

```
modprobe: FATAL: Module usbip-vudc not found
```

**Fixes:**
1. Install kernel headers: `sudo apt install linux-headers-$(uname -r)`, then build module.
2. Use uinput fallback: `sudo modprobe uinput && usbip-client --connect 192.168.1.100:3240 --busid 1-1 --use-uinput`
3. Kernel lacks VHCI -- consider custom kernel or different distro.

### Linux -- Import succeeds but no device appears

```bash
lsmod | grep usbip
dmesg | grep usbip
usbip-client status
```

**Fix:** Client may not have called attach ioctl. Ensure client runs with appropriate permissions.

### Windows -- VHCI driver not installed

```powershell
pnputil /enum-drivers | findstr usbip
pnputil /add-driver "C:\Program Files\AnyPlug\drivers\usbipvhci.inf" /install
```

---

## Android Issues

### Device not detected

**Check:** `adb shell lsusb`. App shows "Local USB Devices"? Try different OTG cable. Check USB settings: "USB controlled by" -> "This device".

### Foreground service stops

Android may kill the service under memory pressure. Disable battery optimization: `Settings -> Apps -> AnyPlug -> Battery -> Unrestricted`.

### "USB device not accessible" on Android 12+

Grant USB permission when prompted, or `Settings -> Connected devices -> USB -> AnyPlug -> Grant`.

### Non-rooted -- uinput issues

```bash
adb shell ls -la /dev/uinput  # missing if kernel lacks CONFIG_INPUT_UINPUT
```

**Limitations:** HID only, no force feedback, some complex HID devices may not fully function.

---

## Windows Issues

### Device not enumerating on server

```powershell
$env:RUST_LOG="debug"
.\usbip-server.exe
# Look for: DEBUG usbip_server::winusb: Found device: 046d:c261
```

**Fixes:** Run as Administrator. Check Device Manager > Human Interface Devices. Try different USB port.

### Firewall blocking connection

```powershell
New-NetFirewallRule -DisplayName "AnyPlug Server" -Direction Inbound -Protocol TCP -LocalPort 3240 -Action Allow
New-NetFirewallRule -DisplayName "AnyPlug Client" -Direction Outbound -Protocol TCP -RemotePort 3240 -Action Allow
```

### GUI tray icon not appearing

Check notification area settings (click arrow in system tray to show hidden icons).

---

## mDNS Discovery Issues

### Server not discoverable

```bash
RUST_LOG=debug usbip-server 2>&1 | grep mdns                          # server registration
RUST_LOG=debug usbip-client --discover 2>&1 | grep mdns               # client browse
avahi-browse -a                                                        # Linux
# Windows: dns-sd -B _usbip._tcp local
```

**Fixes:** Ensure Avahi is running (Linux). mDNS is link-local -- same subnet required. Enterprise Wi-Fi may block mDNS, use direct IP. Windows Firewall may block UDP 5353: `New-NetFirewallRule -DisplayName "mDNS" -Direction Inbound -Protocol UDP -LocalPort 5353 -Action Allow`

### Slow device listing

Client waits 2 seconds by default for mDNS responses. Increase timeout in `discovery.rs` (line 35: `Duration::from_secs(2)`) if servers respond slowly.

---

## Encryption Issues

### "Crypto handshake failed"

- Both sides must use `--encrypt` flag. Mixed mode is rejected.
- Ensure `ring` crate dependencies available: on Linux, `sudo apt install clang libclang-dev`.

### "Decryption failed / authentication tag mismatch"

- Data corruption in transit. Ensure `tcp_nodelay=true` (default).
- Key mismatch: client and server must derive same session key.
- Proxies or load balancers that modify packets will break encryption.

### Performance degradation with encryption

AES-256-GCM adds ~12 bytes nonce + ~16 bytes auth tag per message. On underpowered Android devices, encryption may cause noticeable latency. Consider Ethernet over Wi-Fi. See [PERFORMANCE.md](PERFORMANCE.md#encryption-overhead).

---

## Logs and Debugging

### Enable verbose logging

```bash
RUST_LOG=debug usbip-server    # Levels: error, warn, info, debug, trace
RUST_LOG=trace usbip-server    # URB dumps
```

### Log file locations

| Platform | Location |
|----------|----------|
| Linux (systemd) | `journalctl -u usbip-server -f` |
| Linux (manual) | stderr/stdout |
| Windows (service) | `%PROGRAMDATA%\anyplug\logs\service.log` |
| Windows (GUI) | `%APPDATA%\anyplug\logs\app.log` |
| Android | `adb logcat -s AnyPlug,RustBridge` |

### Common log patterns

```
INFO  usbip_server: Device 046d:c261 exported on bus 1-1
INFO  usbip_server::server: Client connected: 192.168.1.50:54321
TRACE usbip_server::usb: URB seqnum=42 devid=1 ep=0x81 dir=IN len=64
INFO  usbip_server::server: Client disconnected: 192.168.1.50:54321
WARN  usbip_server::server: Import rejected -- device 046d:c261 already exported
```

### Analyzing URB traffic

With `RUST_LOG=trace`, each URB is logged -- verify control transfers and interrupt IN traffic:

```
TRACE usbip_server::usb: URB seqnum=1024 ep=0x01 dir=OUT flags=0x0000 len=8 data=[...]
TRACE usbip_server::usb: URB seqnum=1025 ep=0x81 dir=IN flags=0x0200 actual=64 data=[...]
```

---

## Known Issues

| Issue | Status | Workaround |
|-------|--------|------------|
| Force feedback not working on non-rooted Android | By design (no kernel VHCI) | Use rooted device or Linux client |
| Windows VHCI driver requires reboot after install | Known limitation | Reboot once after first install |
| mDNS not working across VLANs | mDNS protocol limit | Use direct IP: `usbip-client --connect <IP>:3240` |
| Encryption adds latency on Raspberry Pi | Expected (no HW AES) | Disable encryption on LAN |
| Some USB 3.0 ports give enumeration issues on certain devices | Known hardware quirk | Use USB 2.0 port |
