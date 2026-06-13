# usb-passthrough

A cross-platform USB/IP bridge that exports a physical USB device on one machine and imports it on another as if it were locally attached. Built around the USB/IP kernel protocol (RFC-compliant, see PROTOCOL.md).

## Language

**Passthrough**:
True USB passthrough — the device's native descriptors and endpoints are forwarded byte-for-byte over USB/IP, so the OS on the importing side loads the device's real driver. Force feedback, vendor-specific reports, bulk-only storage protocols, and CDC-ACM all work because the device is *not* re-emulated.
_Avoid_: Emulation, redirection, HID proxying, virtualisation

**Server**:
The machine that has the USB device physically attached and exports it over USB/IP. Runs the libusb / WinUSB / Android USB Host backed device-export side.
_Avoid_: Host, exporter, source

**Client**:
The machine that imports an exported USB device and presents it to its local OS as if it were physically attached. On Linux this uses vhci-hcd; on Windows it uses WinUSB; on Android it uses the VHCI module (rooted) or a uinput fallback.
_Avoid_: Guest, importer, consumer, target

**Device class scope (v1.0)**:
HID (keyboards, mice, gamepads, FFB wheels), USB mass storage, USB-to-serial, printers, scanners, and any bulk-only device. Isochronous transfers (USB audio, webcams, full-speed FFB) are explicitly out of scope for v1.0.
_Avoid_: "Works with anything USB" (too broad — implies isoch support), "any USB device" (same)

**G920 debt**:
Code in the shared core that encodes assumptions specific to the Logitech G920 racing wheel (VID/PID constants, FFB command bytes, endpoint layouts). The G920 is the original reference device but the project is no longer G920-specific; any G920-shaped code in `shared/usbip-core/` is a bug to be removed, not a feature to be extended.
_Avoid_: "G920 support" (the project supports arbitrary HID, not a specific wheel), "Logitech quirks" (those are device-profile data, not core logic)

**Test rig**:
The end-to-end test harness built on Linux's raw-gadget + dummy_hcd/udc. A self-hosted GitHub Actions runner loads the kernel modules, presents a software USB device (e.g. a fake HID keyboard) to the host, and the project's own server + client connect to it over loopback. CI proves the tool works with arbitrary USB devices, not just the G920.
_Avoid_: Mock device (too narrow — implies a userspace stub), emulator (ambiguous with VM emulation)

**Reliability primitives**:
The three non-negotiable capabilities for v1.0: structured errors with correlation IDs and exportable logs, hot-plug detection (device attach/detach after server start), and auto-reconnect (survive network flaps and server restarts). Session persistence is explicitly deferred to a later phase.
_Avoid_: Resilience, fault tolerance, recovery (too vague — the project is specific about *which* failures are handled)

**Service mode**:
A headless runtime that survives reboots and requires no UI after initial setup. On Windows: a Windows Service. On Android: a foreground service with a wake lock. On Linux: a systemd unit. The presence of a GUI is a *companion* to service mode, not a replacement for it.
_Avoid_: Daemon (Unix-specific connotation; the project is cross-platform), background app (ambiguous about lifecycle)
