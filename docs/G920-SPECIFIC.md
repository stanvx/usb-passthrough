# Logitech G920 — Specific Configuration Guide

Deep-dive into USB descriptors, endpoints, force feedback, and platform-specific quirks for the Logitech G920 Racing Wheel when used with USB/IP passthrough.

> **VID:** `0x046D` (Logitech)  
> **PID (Xbox variant):** `0xC261`  
> **PID (PC variant):** `0xC262`  
> **USB Speed:** Full (12 Mbps)  
> **G920 detection code:** `windows/src/windows_usb.rs` lines 32-34

---

## Table of Contents

- [Device Overview](#device-overview)
- [USB Descriptors](#usb-descriptors)
- [Endpoints](#endpoints)
- [Force Feedback over USB/IP](#force-feedback-over-usbip)
- [Linux-Specific Configuration](#linux-specific-configuration)
- [Windows-Specific Configuration](#windows-specific-configuration)
- [Android-Specific Notes](#android-specific-notes)
- [Known G920 Quirks](#known-g920-quirks)
- [Testing Your Setup](#testing-your-setup)

---

## Device Overview

The Logitech G920 is a USB HID device with the following characteristics:

| Property | Value |
|----------|-------|
| VID | `0x046D` |
| PID | `0xC261` (Xbox variant) / `0xC262` (PC variant) |
| Device Class | `0x00` (per-interface) |
| USB Speed | Full (12 Mbps) — not High Speed |
| Max Power | 500 mA (bus-powered, no external PSU) |
| Endpoints | 2 (Interrupt IN + Interrupt OUT) |
| HID Report | 78-byte input report, 4-byte output report |

> **Important:** The G920 is a **Full Speed** device (12 Mbps), not High Speed. This means it uses interrupt transfers with short intervals. USB/IP must handle these with low latency for proper force feedback.

### Variant Detection

The two firmware variants are detected by PID:

```
C261 — Xbox variant: works on Xbox and PC, uses XInput
C262 — PC variant: native DirectInput, no Xbox compatibility

Windows enumeration (windows_usb.rs):
  pub const PID_G920_XBOX: u16 = 0xC261;
  pub const PID_G920_PC: u16 = 0xC262;
```

---

## USB Descriptors

### Device Descriptor

```
Offset  Field           Value
0x00    bLength         18
0x01    bDescriptorType 0x01 (DEVICE)
0x02    bcdUSB          0x0200 (USB 2.0)
0x04    bDeviceClass    0x00
0x05    bDeviceSubClass 0x00
0x06    bDeviceProtocol 0x00
0x07    bMaxPacketSize0 64
0x08    idVendor        0x046D
0x0A    idProduct       0xC261 / 0xC262
0x0C    bcdDevice       0x0100
0x0E    iManufacturer   1 ("Logitech")
0x0F    iProduct        2 ("G920 Racing Wheel")
0x10    iSerialNumber   3 (varies)
0x11    bNumConfigurations 1
```

### Configuration Descriptor Summary

```
Configuration 1:
  Interface 0 (HID):
    Endpoint 0x81: Interrupt IN, interval=1ms, maxPacket=78
    Endpoint 0x02: Interrupt OUT, interval=1ms, maxPacket=4
```

Full descriptor tree:

```
Configuration Descriptor:
  bLength: 9
  bDescriptorType: 0x02
  wTotalLength: 0x0029 (41 bytes)
  bNumInterfaces: 1
  bConfigurationValue: 1
  iConfiguration: 0

  Interface Descriptor:
    bLength: 9
    bDescriptorType: 0x04
    bInterfaceNumber: 0
    bAlternateSetting: 0
    bNumEndpoints: 2
    bInterfaceClass: 0x03 (HID)
    bInterfaceSubClass: 0x00
    bInterfaceProtocol: 0x00
    iInterface: 0

  HID Descriptor:
    bLength: 9
    bDescriptorType: 0x21
    bcdHID: 0x0111
    bCountryCode: 0x00
    bNumDescriptors: 1
    bDescriptorType: 0x22 (Report)
    wDescriptorLength: 0x0148 (328 bytes)

  Endpoint Descriptor (IN):
    bLength: 7
    bDescriptorType: 0x05
    bEndpointAddress: 0x81 (IN, ep 1)
    bmAttributes: 0x03 (Interrupt)
    wMaxPacketSize: 0x004E (78 bytes)
    bInterval: 0x01 (1 ms)

  Endpoint Descriptor (OUT):
    bLength: 7
    bDescriptorType: 0x05
    bEndpointAddress: 0x02 (OUT, ep 2)
    bmAttributes: 0x03 (Interrupt)
    wMaxPacketSize: 0x0004 (4 bytes)
    bInterval: 0x01 (1 ms)
```

The protocol types for these are defined in `shared/usbip-core/src/protocol.rs` as `UsbIpDeviceEntry`, and URB structures in `shared/usbip-core/src/urb.rs`.

---

## Endpoints

### Endpoint 0x81 — Interrupt IN

- **Direction:** Device → Host
- **Type:** Interrupt
- **Max Packet Size:** 78 bytes
- **Interval:** 1 ms (polled every USB microframe)
- **Purpose:** Transmits wheel state (position, buttons, paddle shifts)

**Input Report Layout (78 bytes):**

```
Byte 0:      Report ID (0x01)
Bytes 1-2:   Wheel position (signed 16-bit, -32768 to 32767)
Bytes 3-4:   Accelerator pedal (unsigned 16-bit)
Bytes 5-6:   Brake pedal (unsigned 16-bit)
Bytes 7-8:   Clutch pedal (unsigned 16-bit)
Bytes 9:     Buttons (bitmask)
Bytes 10-13: More buttons / D-pad
Bytes 14-77: Padding / vendor-specific
```

When exported via USB/IP, these 78-byte interrupt IN URBs are serialized as `UsbIpRetSubmit` messages (defined in `urb.rs`):

```rust
pub struct UsbIpRetSubmit {
    pub seqnum:            U32BE,
    pub devid:             U32BE,
    pub direction:         U32BE,
    pub ep:                U32BE,
    pub status:            U32BE,
    pub actual_length:     U32BE,    // 78 bytes for G920 IN reports
    pub start_frame:       U32BE,
    pub number_of_packets: U32BE,
    pub error_count:       U32BE,
    pub setup:             [u8; 8],
    // variable data follows: 78 bytes
}
```

### Endpoint 0x02 — Interrupt OUT

- **Direction:** Host → Device
- **Type:** Interrupt
- **Max Packet Size:** 4 bytes
- **Interval:** 1 ms
- **Purpose:** Force feedback commands, LED control

**Output Report Layout (4 bytes):**

```
Byte 0:      Report ID (0x01)
Bytes 1-3:   Force feedback command payload
```

These are sent as `UsbIpCmdSubmit` from the client to the server, which forwards them to the physical device.

---

## Force Feedback over USB/IP

Force feedback is the most demanding aspect of the G920 passthrough.

### How FF Works

1. The host application (game/sim) sends force feedback effects via the HID output endpoint.
2. On the server side, the physical wheel receives these as Interrupt OUT URBs.
3. The wheel's internal microcontroller applies the force.
4. Interrupt IN URBs carry the wheel's current position back to the application.

### Over USB/IP

The URB flow for force feedback:

```
┌──────────┐  CMD_SUBMIT (ep=0x02, OUT, 4 bytes)  ┌──────────┐  Interrupt OUT  ┌──────┐
│  Client  │ ───────────────────────────────────► │  Server  │ ──────────────► │ G920 │
│  (Game)  │                                       │          │                 │      │
│          │ ◄─────────────────────────────────── │          │ ◄────────────── │      │
│          │  RET_SUBMIT (ep=0x81, IN, 78 bytes)  │  Server  │  Interrupt IN   │      │
└──────────┘                                       └──────────┘                 └──────┘
```

### Latency Requirements

Force feedback is sensitive to latency. The G920 polls at 1 ms intervals (1000 Hz).

| Latency | Experience |
|---------|------------|
| < 2 ms | Native feel, excellent |
| 2-5 ms | Good, minor delay in FF |
| 5-10 ms | Noticeable lag, playable |
| 10-20 ms | Poor, FF feels sluggish |
| > 20 ms | Unusable for sim racing |

**Recommendations:**
- Use wired Ethernet (not Wi-Fi) for best latency.
- Enable `tcp_nodelay` (enabled by default in both server and client).
- Keep the network path short — avoid VPNs, proxies, or NAT traversal.
- See [PERFORMANCE.md](PERFORMANCE.md) for detailed latency tuning.

### URB Pool Sizing for G920

The G920 generates URBs at up to 1000 per second (every USB frame). The URB buffer pool in `urb.rs` uses pre-allocated buffers:

```rust
pub struct UrbBuffer {
    pub buf: Vec<u8>,           // Pre-allocated wire message buffer
    pub data_offset: usize,     // Where payload starts
    pub data_capacity: usize,   // Capacity for data
}
```

Default capacity: 1024 entries. For the G920's 78-byte IN reports + 4-byte OUT reports, this handles ~1 second of full-rate traffic before recycling.

---

## Linux-Specific Configuration

### udev Rules

Create `/etc/udev/rules.d/99-g920.rules`:

```
# G920 Xbox variant
SUBSYSTEM=="usb", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c261", MODE="0666", GROUP="plugdev"
# G920 PC variant
SUBSYSTEM=="usb", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c262", MODE="0666", GROUP="plugdev"
```

Reload:

```bash
sudo udevadm control --reload-rules
sudo udevadm trigger
```

### Kernel Modules

The G920 uses the `hid_logitech` kernel module for HID support, and `ff-memless` for force feedback:

```bash
# Load required modules
sudo modprobe hid_logitech
sudo modprobe ff-memless
sudo modprobe usbip-vudc  # for client VHCI
```

### Testing on Linux

```bash
# Check if the wheel is detected locally
lsusb | grep Logitech

# Verify HID device
ls -la /dev/input/by-id/*G920*

# Test force feedback
sudo apt install fftest
sudo fftest /dev/input/by-id/usb-Logitech_G920_Racing_Wheel-event-joystick

# Monitor URB traffic from server
RUST_LOG=trace usbip-server --allow 046d:c261 2>&1 | grep "URB.*ep=0x81"
```

### VHCI Import on Linux

```bash
# Load VHCI
sudo modprobe usbip-vudc

# Connect client
sudo usbip-client --connect <SERVER_IP>:3240 --busid 1-1

# Verify the imported device
lsusb | grep Logitech

# Check input devices
evtest
```

---

## Windows-Specific Configuration

### Driver Notes

On Windows, the G920 uses:
- **Xbox variant (C261):** XInput driver (native Xbox compatibility)
- **PC variant (C262):** DirectInput driver

The USB/IP server on Windows uses SetupAPI to enumerate devices (see `windows/src/windows_usb.rs`):

```rust
pub const VID_LOGITECH: u16 = 0x046D;
pub const PID_G920_XBOX: u16 = 0xC261;
pub const PID_G920_PC: u16 = 0xC262;
```

### Windows Firewall

Allow the server through the firewall:

```powershell
New-NetFirewallRule -DisplayName "USB Passthrough - G920" `
  -Direction Inbound -Protocol TCP -LocalPort 3240 -Action Allow
```

### Testing on Windows

```powershell
# List devices (run as admin)
usbip-server list

# Run the GUI and check the G920 appears
.\usbip-server-windows.exe

# Enable debug logging
$env:RUST_LOG = "debug"
.\usbip-server.exe
```

---

## Android-Specific Notes

### Server Mode

When running the server on Android, the G920 must be connected via USB OTG:

1. Connect the G920 to your Android device via OTG cable.
2. Grant USB permission when prompted.
3. In the app, tap "Share" in the Server tab.

**Important for Android:**
- The G920 draws up to 500 mA. Some Android devices may not supply enough power via OTG.
- If the wheel powers on but disconnects under load (force feedback), use a powered USB hub between the phone and the wheel.
- On Android, `UsbPassthroughService` acquires a partial wake lock to keep the CPU active:
  ```kotlin
  val pm = getSystemService(POWER_SERVICE) as PowerManager
  wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "usb-passthrough:wakelock")
  ```

### Client Mode

On rooted Android with VHCI support, the G920 can be imported. On non-rooted:
- Only HID input (wheel position, buttons) may work via uinput.
- Force feedback will **not** work without kernel VHCI.

---

## Known G920 Quirks

### 1. USB 3.0 Port Issues

The G920 is a Full Speed (12 Mbps) device. Some USB 3.0 controllers have compatibility issues with it.

**Symptoms:**
- Wheel enumerates and de-enumerates repeatedly.
- `dmesg` shows: `USB disconnect, device number X`

**Fix:** Always use a **USB 2.0 port**. If your PC only has USB 3.0 ports, use a USB 2.0 hub.

### 2. Power Cycling Required

If the server starts before the wheel is fully initialized, the wheel may not be detected.

**Fix:** Unplug and replug the wheel while the server is running, or start the server after the wheel is connected.

### 3. Two G920 PID Variants

The Xbox variant (C261) and PC variant (C262) have different firmware. Exporting one variant and importing on another platform works, but:
- XInput features (guide button, Xbox button mapping) are only available on the C261 variant.
- The PC variant (C262) works with DirectInput games natively.

### 4. Force Feedback Initialization

Some games require a specific FF initialization sequence at startup. If FF doesn't work:

```bash
# Linux: check if FF is available
cat /sys/class/input/event*/device/ff
# Should output the number of FF effects supported

# Test with fftest
sudo fftest /dev/input/by-path/pci-*-usb-*-event-joystick
```

If FF is not working over USB/IP:
- Verify the URB pool isn't dropping OUT URBs (check trace logs).
- Ensure `tcp_nodelay` is enabled.
- Try reducing network latency (see [PERFORMANCE.md](PERFORMANCE.md)).

### 5. Spring Constant Effect

The G920 has a built-in spring constant (centering spring) that is controlled by the driver. Over USB/IP, if the client OS doesn't send the spring constant command periodically, the wheel will feel loose.

**Fix:** Most sim games send spring/centering commands automatically. If not, a background utility may be needed.

### 6. Combined Pedal Report

The G920 reports accelerator, brake, and clutch pedals in a single 78-byte HID report. This means all three pedal values arrive atomically — there's no per-pedal endpoint.

---

## Testing Your Setup

### Quick Connectivity Test (Linux)

```bash
# Server side
sudo usbip-server --allow 046d:c261  # or c262

# Client side (on same or different machine)
sudo usbip-client --list <SERVER_IP>:3240
# Expected:   046d:c261  1-1  Logitech G920 Racing Wheel

sudo usbip-client --connect <SERVER_IP>:3240 --busid 1-1
```

### Verify Force Feedback (Linux client)

```bash
sudo apt install joystick fftest
sudo fftest /dev/input/by-id/usb-*-event-joystick
```

In `fftest`:
- Press `1` for constant force.
- Move the wheel — it should resist movement.
- Press `0` to stop.

### Verify HID Report (Windows server)

With `RUST_LOG=trace`, the server logs every URB:

```
TRACE usbip_server::usb: URB seqnum=1 ep=0x81 dir=IN flags=0x0200 actual=78
  data=[01 00 40 00 00 00 00 00 ...]
```

The first two data bytes (`00 40`) are the wheel position (little-endian, 0x4000 = 16384 = center).

---

## Configuration File

If you run the server regularly for the G920, save a config file at `/etc/usbip-server/config.toml`:

```toml
bind_address = "0.0.0.0"
port = 3240
tcp_nodelay = true
encryption_enabled = false

# Only allow G920 devices
allowed_devices = [
    { vid = 0x046d, pid = 0xc261 },  # G920 Xbox
    { vid = 0x046d, pid = 0xc262 },  # G920 PC
]
```

> Note: CLI args override config file values when both are present.

---

## References

- `windows/src/windows_usb.rs` — Windows enumeration and G920 detection
- `shared/usbip-core/src/protocol.rs` — USB/IP protocol constants
- `shared/usbip-core/src/urb.rs` — URB types for interrupt transfers
- `shared/usbip-core/src/lib.rs` — Speed enum, version constants
- USB.org: HID specification
- Linux kernel: `hid-logitech` driver documentation
