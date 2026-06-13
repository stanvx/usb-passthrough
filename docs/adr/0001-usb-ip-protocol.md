# USB/IP protocol as the wire format

The system speaks the Linux kernel's USB/IP protocol (USB/IP v1.1.1, see PROTOCOL.md) for the server-client transport rather than a custom protocol or a proprietary alternative like VirtualHere or usbip-virtualization. The protocol is RFC-documented, has been upstream in the Linux kernel since 2008, and is implemented in `shared/usbip-core/src/protocol.rs`. The decision buys interoperability with the kernel's own `usbip-host` and `vhci-hcd` modules (so a usb-passthrough server can be consumed by a stock Linux client and vice versa) and a battle-tested URB submission / completion flow that already handles cancellation, error propagation, and partial transfers correctly.

## Considered Options

- **Custom wire protocol** — rejected. Reimplementing URB forwarding, error codes, and cancellation correctly is a multi-quarter project. The kernel's USB/IP has been in production for 17 years and is the lingua franca of the niche.
- **VirtualHere protocol** — rejected. Proprietary, single-vendor (the VirtualHere server is closed-source), and licence-restricted. The moment we adopt it, the project becomes a client of one company.
- **USB/IP (chosen)** — chosen. Standard, documented, open-source reference implementations exist, and the protocol is small enough to implement in pure Rust without libusb-bound code (which is what `shared/usbip-core` does).
