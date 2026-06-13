# v1.0 device-class scope: HID + storage + bulk-only, isoch deferred

The v1.0 release supports interrupt and bulk USB transfers only. Isochronous transfers — required for USB audio (DACs, headsets, microphones), webcams, and full-speed force feedback at 1 kHz+ — are explicitly out of scope and deferred to a later phase. The reason is structural: isochronous requires bandwidth reservation, latency-jitter handling, and (for video) sustained high-throughput plumbing that is incompatible with the current "forward URB over TCP" transport as designed. Adding it is a quarter-scale protocol expansion, not a feature toggle. HID + storage + bulk-only covers the actual use cases of the target audience (gamers and prosumer/home users) and represents ~90% of the USB peripherals a household owns: keyboards, mice, gamepads, FFB wheels, flash drives, external SSDs, printers, scanners, USB-to-serial adapters, and CDC-ACM modems. Audio devices that present as a basic HID (volume keys, play/pause) still work; they just don't work as a *sound card*. Webcams do not work. Devices that are wholly isochronous should not be attempted with v1.0.

## Consequences

- The test rig's first gadget is a fake HID keyboard (interrupt transfer) and a fake mass storage function (bulk transfer). Audio/webcam gadgets are not part of v1.0 CI.
- Documentation must be honest about this. The README's "Works with anything USB" claim is too broad for v1.0 and should be rephrased.
- The protocol core is unchanged by this decision — `UsbIpCmdSubmit` already encodes the transfer type. Adding isoch is a runtime/transport concern, not a wire-format concern.
