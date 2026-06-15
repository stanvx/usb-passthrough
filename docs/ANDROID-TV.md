# AnyPlug — Android TV Guide

Using USB/IP passthrough on Android TV / Google TV devices to share or import USB game controllers and input devices.

> **Target Devices:** NVIDIA Shield TV, Google Chromecast with Google TV, Sony Bravia TVs (Android TV), Xiaomi Mi Box, and other Android TV 10+ devices.
> **Module:** `android/tv/` — Compose UI for TV with d-pad navigation

---

## Compatibility

### Supported Devices

| Device | USB Host | OTG | Notes |
|--------|----------|-----|-------|
| NVIDIA Shield TV Pro (2019) | Full | Yes | Best option — multiple USB ports |
| NVIDIA Shield TV (2017) | Full | Yes | USB 3.0 + Micro-USB OTG |
| Google Chromecast w/ Google TV | Limited | Needs powered hub | Only 1 USB-C port |
| Sony Bravia (Android TV 10+) | Full | Built-in USB-A | Check model specs |
| Xiaomi Mi Box S | Limited | USB 2.0 only | May under-power high-power devices |
| TCL / Hisense Android TVs | Varies | Varies | Check USB port power rating |

### Requirements

- Android TV OS 10+ (API 29)
- USB Host mode support (standard on most Android TV devices)
- 2 GB RAM minimum (4 GB recommended)
- USB OTG cable if device uses Micro-USB / USB-C

### Power

| Device | USB Power |
|--------|-----------|
| Shield TV Pro | 900 mA per port — works fine |
| Chromecast w/ Google TV | ~500 mA via USB-C — borderline, may need powered hub |
| Sony Bravia | 500 mA — usually works |
| Mi Box S | Often insufficient |

If the device powers on but disconnects during operation, use a **powered USB hub**.

---

## Installation

Sideload the APK — Android TV does not have a Play Store listing.

```bash
# 1. Enable Developer Options:
#    Settings → Device Preferences → About → Build → Tap 7 times

# 2. Enable "Unknown sources" and "USB debugging":
#    Settings → Device Preferences → Developer Options → Enable both

# 3. Get TV's IP: Settings → Network & Internet

# 4. Install via ADB
adb connect <TV_IP>:5555
adb install anyplug-tv-release.apk

# 5. Verify
adb shell pm list packages | grep anyplug
```

### Via USB Drive

Copy `anyplug-tv-release.apk` to a USB drive, insert into TV, use a file manager to tap the APK.

### ADB Over Network (No USB Debugging)

```bash
adb connect <TV_IP>:5555
```

---

## TV Server Mode

Export a locally-connected racing wheel to the network.

1. Connect wheel via USB (or USB OTG + powered hub).
2. Open "AnyPlug TV" app.
3. Select device from "Local USB Devices" list.
4. Press "Share" or center button on remote.

The app starts a foreground service (`AnyPlugService`) with persistent notification "AnyPlug — Server running". Server begins mDNS advertisement (`_usbip._tcp.local.`) and listens on TCP 3240.

The server holds a `PARTIAL_WAKE_LOCK` to keep CPU active even with screen off. This increases power consumption.

---

## TV Client Mode

Import a remote device to your Android TV.

1. Ensure a USB/IP server is running on the network.
2. Open "AnyPlug TV" app, navigate to Client tab.
3. Select discovered server or enter manual address.
4. Select device, press "Connect".

The foreground service starts. Connectivity via TCP 3240. If the TV is **rooted**: VHCI attaches the device as full USB (includes force feedback). If **not rooted**: uinput fallback creates an input device (HID only, no force feedback).

---

## Remote Control Navigation

| Button | Action |
|--------|--------|
| Up/Down | Navigate list items |
| Left/Right | Switch tabs (Server/Client) |
| Center/OK | Select focused item or button |
| Back | Go back / disconnect |
| Home | Return to home (service continues) |

Voice search is not supported.

---

## Foreground Service on TV

Critical on Android 8+ to prevent the OS from killing the process.

- **Start:** When you select "Share" or "Connect".
- **Background:** Service continues on Home press. Notification visible.
- **Stop:** Select "Stop" in the app or disconnect explicitly.
- **Kill:** Swiping from recent tasks does not stop the service on most Android TV versions.
- **Crash:** `START_STICKY` return value means Android restarts the service if it crashes.

---

## Known Limitations

### Non-Rooted TV — No Force Feedback

Most Android TV devices are not rooted. The uinput fallback provides:
- Wheel position and rotation
- Pedal inputs (gas, brake, clutch)
- Buttons and D-pad

But no force feedback or centering spring. **Workaround:** Use TV as server (export wheel) and connect from a PC client. For TV client, force feedback requires root.

### USB Power

Some devices may draw more power than the TV's port supplies — especially during force feedback peaks. Symptoms: wheel disconnects, resets during calibration, intermittent USB enumeration. Fix: use a powered USB hub.

### mDNS on Android TV

mDNS works on the same subnet only. Guest networks may block multicast. Use manual connection if servers aren't discovered.

### Limited Storage

APK is ~8 MB, but logs accumulate at `/data/data/com.anyplug/files/logs/`. Clean up periodically.

---

## Troubleshooting

### App Won't Install

```bash
adb shell getprop ro.build.version.sdk       # Must be >= 29
adb shell df /data                            # Check storage
adb shell uname -m                            # Check architecture (ARM64 expected)
```

### USB Device Not Detected

```bash
adb shell lsusb
adb shell cat /sys/kernel/debug/usb/devices
adb shell dmesg | grep -i usb
```

### Cannot Connect to Server

```bash
adb shell ping <SERVER_IP>
adb shell nc -zv <SERVER_IP> 3240
adb logcat -s AnyPlug
```

### Service Keeps Stopping

```bash
adb shell dumpsys meminfo com.anyplug
adb shell dumpsys activity processes | grep anyplug
```

### Force Feedback Not Working on Rooted TV

```bash
adb shell lsmod | grep usbip
adb shell zcat /proc/config.gz | grep USBIP_VUDC
```

---

## Building the TV APK

See [BUILDING.md](BUILDING.md) for detailed instructions.

```bash
cd /home/localadmin/anyplug/android
./gradlew :tv:assembleRelease
# Output: android/tv/build/outputs/apk/release/anyplug-tv-release.apk
```

---

## Use Cases

| Use Case | Setup | Force Feedback |
|----------|-------|----------------|
| Best: TV as Server | Wheel plugged into TV, TV exports to PC running sim game | Full FFB on PC client |
| OK: TV as Client (Rooted) | PC server with wheel, TV runs sim game with VHCI | Full FFB on TV |
| Limited: TV as Client (Non-Rooted) | PC server, TV runs sim game, uinput fallback | No FFB, wheel + buttons only |

---

## References

- `android/tv/src/main/java/com/anyplug/tv/TvMainActivity.kt` — TV UI entry point
- `android/app/src/main/java/com/anyplug/AnyPlugService.kt` — Foreground service
- `android/app/src/main/java/com/anyplug/client/UsbIpClient.kt` — Client protocol implementation
- `android/app/src/main/java/com/anyplug/server/UsbIpServer.kt` — Server protocol implementation
