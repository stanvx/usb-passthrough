# anyplug

A cross-platform USB/IP bridge that exports a physical USB device on one machine and imports it on another as if it were locally attached. Built around the USB/IP kernel protocol (RFC-compliant, see PROTOCOL.md).

## Language

**Passthrough**:
Byte-for-byte forwarding of native USB descriptors and endpoints over USB/IP.
The importing OS loads the real driver, so FFB, vendor reports, bulk storage,
and CDC-ACM all work — the device is not re-emulated.
_Avoid_: Emulation, redirection, HID proxying, virtualisation

**Server**:
Machine with the physically attached USB device, exporting it over USB/IP
via libusb / WinUSB / Android USB Host.
_Avoid_: Host, exporter, source

**Client**:
Machine that imports an exported USB device and presents it as locally attached.
Linux: vhci-hcd. Windows: WinUSB. Android: VHCI module (rooted) or uinput fallback.
_Avoid_: Guest, importer, consumer, target

**Device class scope (v1.0)**:
HID, mass storage, USB-to-serial, printers, scanners, bulk-only devices.
Isochronous (audio, webcams) is out of scope.
_Avoid_: "Works with anything USB" (implies isoch support)

**Test rig**:
QEMU-based E2E harness on cloud CI runners. Linux configfs + dummy_hcd/udc
presents software USB devices (HID, mass storage, CDC-ACM) over loopback
to the project's own server + client.
_Avoid_: Mock device (too narrow), emulator (ambiguous with VM)

**Reliability primitives**:
Three v1.0 requirements: structured errors with correlation IDs, hot-plug
detection, auto-reconnect. Session persistence is deferred.
_Avoid_: Resilience, fault tolerance, recovery (too vague)

**Ecosystem integration**:
Packaging existing binaries for community platforms via native mechanisms
(RetroPie scriptmodule, Lakka package, Steam Link/Moonlight companion).
No new code.
_Avoid_: Feature development, new UI, protocol changes

**RetroPie / Lakka integration**:
The RetroPie/Lakka device runs `usbip-server` to export controllers and
`usbip-client` to import remote devices. Distribution-only — no new code.
_Avoid_: "RetroPie support", "Lakka support" (implies new features)

**Steam Link / Moonlight companion**:
Streaming-client device (Pi at the TV) runs `usbip-server` to export
controllers to the gaming PC. The PC runs `usbip-client` to import them.
Same architecture, deployment-specific packaging.
_Avoid_: "Steam integration" (not a plugin), "Moonlight plugin" (companion service, not a fork)

**Client daemon**:
Persistent background service — auto-starts on boot, auto-connects,
survives login/logout. Linux: systemd unit with control socket.
Windows: Windows Service. Android: foreground service. Required for
headless ecosystem integrations.
_Avoid_: Client service (ambiguous), background mode (too vague)

**Embedded server recipe**:
Documented procedure for setting up `usbip-server` on a Raspberry Pi/SBC
as a headless USB-over-network appliance. Stock OS, no custom firmware.
Buildroot/Yocto image deferred post-v1.0.
_Avoid_: CloudHub clone (implies custom firmware), embedded firmware (overstates it)

**Service mode**:
Headless runtime that survives reboots with no UI after setup.
Windows Service / Android foreground service / systemd unit.
GUI is a companion, not a replacement.
_Avoid_: Daemon (Unix-specific), background app (ambiguous)
