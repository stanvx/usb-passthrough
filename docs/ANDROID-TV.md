# USB/IP Passthrough — Android TV Guide

Using USB/IP passthrough on Android TV / Google TV devices to share or import USB racing wheels.

> **Target Devices:** NVIDIA Shield TV, Google Chromecast with Google TV, Sony Bravia TVs (Android TV), Xiaomi Mi Box, and other Android TV 10+ devices.  
> **Module:** `android/tv/` — Compose UI for TV with d-pad navigation

---

## Table of Contents

- [Compatibility](#compatibility)
- [Installation](#installation)
- [TV Server Mode](#tv-server-mode)
- [TV Client Mode](#tv-client-mode)
- [Remote Control Navigation](#remote-control-navigation)
- [Foreground Service on TV](#foreground-service-on-tv)
- [Known TV Limitations](#known-tv-limitations)
- [Troubleshooting on TV](#troubleshooting-on-tv)

---

## Compatibility

### Supported Android TV Devices

| Device | USB Host | OTG Support | Notes |
|--------|----------|-------------|-------|
| NVIDIA Shield TV Pro (2019) | ✅ Full | ✅ | Best option — multiple USB ports |
| NVIDIA Shield TV (2017) | ✅ Full | ✅ | USB 3.0 + Micro-USB OTG |
| Google Chromecast w/ Google TV | ⚠️ Limited | ⚠️ Needs powered hub | Only 1 USB-C port |
| Sony Bravia (Android TV 10+) | ✅ Full | Built-in USB-A | Check model specs |
| Xiaomi Mi Box S | ⚠️ Limited | ⚠️ USB 2.0 only | May under-power G920 |
| TCL / Hisense Android TVs | Varies | Varies | Check USB port power rating |

### Minimum Requirements

- **Android TV OS version:** 10 (API 29) or higher
- **USB Host mode:** Must be supported (most Android TV devices have this)
- **RAM:** 2 GB minimum (4 GB recommended for simultaneous game + passthrough)
- **USB OTG cable:** Required if the device only has Micro-USB / USB-C

### G920 Power Requirements

The G920 draws up to 500 mA. Some Android TV devices provide limited USB power:

```
Shield TV Pro:     ✅ 900 mA per port — works fine
Chromecast w/ GT:  ⚠️ ~500 mA via USB-C — borderline, may need powered hub
Sony Bravia USB:   ✅ 500 mA — usually works
Mi Box S:          ❌ Often insufficient power
```

**If the wheel powers on but disconnects during force feedback:** use a **powered USB hub** between the TV and the wheel.

---

## Installation

### Sideload the APK

Android TV devices don't have the Google Play Store listing for this app, so you need to sideload.

```bash
# 1. Enable Developer Options on your Android TV:
#    Settings → Device Preferences → About → Build → Tap 7 times

# 2. Enable "Unknown sources" and "USB debugging":
#    Settings → Device Preferences → Developer Options → 
#    - Install unknown apps → Enable your file manager
#    - USB debugging → Enable

# 3. Get your TV's IP address:
#    Settings → Network & Internet → (your network)

# 4. Install via ADB
adb connect <TV_IP_ADDRESS>:5555
adb install usb-passthrough-tv-release.apk

# 5. Verify
adb shell pm list packages | grep usbpassthrough
```

### Using a USB Drive

1. Copy `usb-passthrough-tv-release.apk` to a USB drive.
2. Insert into the TV.
3. Use a file manager app (like X-plore File Manager) to navigate to the USB drive.
4. Tap the APK to install.

### ADB over Network (No USB Debugging)

If USB debugging is unavailable, you can sometimes install via ADB over network:

```bash
# Connect to TV
adb connect <TV_IP>:5555

# If connection fails, check TV is on same network
# and developer options / ADB debugging is enabled
```

---

## TV Server Mode

Run the server on your Android TV to export a locally-connected G920 wheel to the network.

### Setup

1. Connect the G920 wheel to your Android TV via USB (or USB OTG + powered hub).
2. Open the "USB Passthrough TV" app from the Android TV home screen.
3. You'll see the main interface optimized for d-pad navigation.
4. Select the G920 wheel from the "Local USB Devices" list.
5. Press **"Share"** or the center button on your remote.

### What Happens

1. The app starts a **foreground service** (`UsbPassthroughService`).
2. A persistent notification appears: "USB Passthrough — Server running".
3. The server begins mDNS advertisement (`_usbip._tcp.local.`).
4. The server listens on TCP port 3240.

### Status Indicators

The TV interface shows:

```
┌──────────────────────────────┐
│  USB Passthrough             │
│                              │
│  ● Running — Server mode     │  ← Green when active
│  Sharing: G920 Racing Wheel  │
│  IP: 192.168.1.50:3240       │
│                              │
│  ╔══════════════════════╗    │
│  ║     Stop Server      ║    │  ← D-pad selectable
│  ╚══════════════════════╝    │
└──────────────────────────────┘
```

### Wake Lock

The server holds a partial wake lock to prevent the TV from sleeping:

```kotlin
// UsbPassthroughService.kt
wakeLock = pm.newWakeLock(
    PowerManager.PARTIAL_WAKE_LOCK,
    "usb-passthrough:wakelock"
)
```

This keeps the CPU active even if the screen turns off.

**Note:** This will increase power consumption. If the TV is used for other purposes while the server is running, expect higher electricity usage.

---

## TV Client Mode

Import a remote G920 wheel to your Android TV. The wheel will appear as if locally connected.

### Setup

1. Ensure a USB/IP server is running on the network (Linux PC, Windows PC, or another Android device).
2. Open the "USB Passthrough TV" app.
3. Navigate to the **Client** tab.
4. Select a discovered server from the list, or enter a manual server address.
5. Select the device you want to import.
6. Press **"Connect"**.

### Client Behavior

- The foreground service starts with a persistent notification.
- The app connects to the remote server via TCP 3240.
- If the TV is **rooted**: VHCI is available and the device attaches as a full USB device.
- If the TV is **not rooted**: uinput fallback creates an input device (HID only, no force feedback).

### Client Status

```
┌──────────────────────────────┐
│  USB Passthrough             │
│                              │
│  ● Running — Client mode     │
│  Connected to:               │
│  192.168.1.100:3240          │
│  Device: G920 Racing Wheel   │
│                              │
│  ╔══════════════════════╗    │
│  ║    Disconnect        ║    │
│  ╚══════════════════════╝    │
└──────────────────────────────┘
```

---

## Remote Control Navigation

The Android TV module (`android/tv/src/main/java/com/usbpassthrough/tv/TvMainActivity.kt`) uses Compose UI optimized for d-pad remotes.

### Navigation Map

```
┌───────────────────────────────────┐
│      USB Passthrough TV          │
│  ┌─────────────────────────────┐ │
│  │  [Server]    [Client]      │ │  ← Tab navigation (Left/Right)
│  └─────────────────────────────┘ │
│                                   │
│  Local USB Devices:               │
│  ┌─────────────────────────────┐ │
│  │ ▶ G920 Racing Wheel        │ │  ← Focusable (Up/Down)
│  │   046d:c261                │ │
│  │   [Share]                  │ │  ← Selectable button
│  └─────────────────────────────┘ │
│                                   │
│  ╔═══════════════════════════╗   │
│  ║    Refresh Devices       ║   │  ← Bottom action
│  ╚═══════════════════════════╝   │
└───────────────────────────────────┘
```

### Remote Button Mapping

| Remote Button | Action |
|---------------|--------|
| **Up/Down** | Navigate list items |
| **Left/Right** | Switch tabs (Server/Client) |
| **Center/OK** | Select focused item or button |
| **Back** | Go back / disconnect |
| **Home** | Return to Android TV home (service continues) |

### Voice Control

Android TV voice search is not supported for this app. All interactions are via remote control.

---

## Foreground Service on TV

The app runs as a foreground service (`UsbPassthroughService`), which is critical on Android 8+ to prevent the OS from killing it.

### Notification Channel

```kotlin
val notification = NotificationCompat.Builder(this, "usb_passthrough_channel")
    .setContentTitle("USB Passthrough")
    .setContentText("Service running")
    .setSmallIcon(android.R.drawable.ic_menu_share)
    .setOngoing(true)
    .setPriority(NotificationCompat.PRIORITY_LOW)
    .build()
startForeground(1001, notification)
```

### Service Lifecycle

- **Start:** When you select "Share" or "Connect", the service starts.
- **Background:** If you press Home, the service continues. Notification is visible.
- **Stop:** Select "Stop" in the app, or the service stops when you explicitly disconnect.
- **Kill:** Swiping away the app from recent tasks **will not** stop the service on most Android TV versions.
- **Crash:** The `START_STICKY` return value means Android will restart the service if it crashes.

### Battery Optimization

On Android TV, battery optimization shouldn't be an issue (TV is plugged in), but the app still requests a `PARTIAL_WAKE_LOCK`.

---

## Known TV Limitations

### ⚠️ Non-Rooted TV — No Force Feedback

Most Android TV devices are **not rooted**. Without root, VHCI is unavailable, and the uinput fallback only provides basic HID input:

- ✅ Wheel position and rotation
- ✅ Pedal inputs (gas, brake, clutch)
- ✅ Buttons and D-pad
- ❌ **Force feedback will NOT work**
- ❌ **Wheel centering spring will NOT work**
- ❌ Some HID features may be missing

**Workaround:** Use the TV as a **server** (exporting a locally-connected wheel) and connect to it from a PC client that supports force feedback. For TV client usage, force feedback requires root.

### ⚠️ USB Power

As noted in [Compatibility](#compatibility), some Android TV devices provide limited USB power. The G920, especially during force feedback peaks, may draw more than the port supplies.

**Symptoms of power issues:**
- Wheel disconnects when force feedback engages
- Wheel calibrates but then resets
- Intermittent USB enumeration

**Fix:** Use a powered USB hub.

### ⚠️ mDNS on Android TV

mDNS discovery works on Android TV only if the network allows multicast. Some considerations:
- mDNS works on the same subnet only.
- Guest networks may block mDNS.
- Use manual connection if servers aren't discovered.

### ⚠️ Limited Storage

Android TV devices often have limited storage. The APK is ~8 MB, but logs may accumulate. The app writes logs to:
```
/data/data/com.usbpassthrough/files/logs/
```
Clean up old logs periodically if storage is limited.

---

## Troubleshooting on TV

### App Won't Install

```bash
# Check Android TV OS version
adb shell getprop ro.build.version.sdk
# Must be >= 29 (Android 10)

# Check for storage space
adb shell df /data

# If APK install fails with INSTALL_FAILED_NO_MATCHING_ABIS
# The TV is likely not ARM64 — check architecture
adb shell uname -m
```

### USB Device Not Detected

```bash
# Check if the TV sees the device
adb shell lsusb
# If lsusb not available:
adb shell cat /sys/kernel/debug/usb/devices

# Check dmesg for USB errors
adb shell dmesg | grep -i usb
```

### Cannot Connect to Server

```bash
# Can the TV reach the server?
adb shell ping <SERVER_IP>

# Is port 3240 open?
adb shell nc -zv <SERVER_IP> 3240

# Check app logs
adb logcat -s UsbPassthrough
```

### Service Keeps Stopping

```bash
# Check for low memory
adb shell dumpsys meminfo com.usbpassthrough

# Check if watchdog is killing the service
adb shell dumpsys activity processes | grep usbpassthrough
```

### Force Feedback Not Working on Rooted TV

```bash
# Verify VHCI is loaded
adb shell lsmod | grep usbip

# If VHCI module is missing, the kernel may not support it
# Check kernel config
adb shell zcat /proc/config.gz | grep USBIP_VUDC
```

---

## Building the TV APK

See [BUILDING.md](BUILDING.md) for detailed build instructions.

Quick reference:

```bash
# Build the TV module
cd /home/localadmin/usb-passthrough/android
./gradlew :tv:assembleRelease

# Sign the APK
./gradlew :tv:signRelease

# Output at: android/tv/build/outputs/apk/release/usb-passthrough-tv-release.apk
```

---

## Use Cases

### Best: TV as Server

```
┌──────────┐   USB/IP   ┌──────────┐
│ Android  │ ◄─────────►│ PC with  │
│ TV       │   TCP 3240 │ sim game │
│ (G920    │            │ (client) │
│  plugged)│            │          │
└──────────┘            └──────────┘
```

- TV is near the sim rig.
- PC in another room handles the game.
- Full force feedback on PC client.

### OK: TV as Client (Rooted)

```
┌──────────┐   USB/IP   ┌──────────┐
│ PC/Laptop│ ◄─────────►│ Android  │
│ (server, │   TCP 3240 │ TV with  │
│ G920     │            │ VHCI     │
│ plugged) │            │ (client) │
└──────────┘            └──────────┘
```

- TV runs the sim game.
- Force feedback works (VHCI on rooted TV).

### Limited: TV as Client (Non-Rooted)

- TV runs the sim game.
- No force feedback — only wheel position and buttons.
- Use for casual racing games that don't require FFB.

---

## References

- `android/tv/src/main/java/com/usbpassthrough/tv/TvMainActivity.kt` — TV UI entry point
- `android/app/src/main/java/com/usbpassthrough/UsbPassthroughService.kt` — Foreground service
- `android/app/src/main/java/com/usbpassthrough/ui/MainScreen.kt` — Base UI components
- `android/app/src/main/java/com/usbpassthrough/client/UsbIpClient.kt` — Client protocol implementation
- `android/app/src/main/java/com/usbpassthrough/server/UsbIpServer.kt` — Server protocol implementation
