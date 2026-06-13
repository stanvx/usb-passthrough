# USB Descriptor Fixtures

This directory holds captured USB descriptor trees used by the integration tests
in `tests/descriptor_fixtures.rs`. Each subdirectory is a single fixture.

## Fixture Layout

```
tests/fixtures/<fixture_name>/
    descriptor.bin      -- raw USB descriptor tree bytes
    metadata.toml       -- TOML sidecar declaring expected properties
```

### `descriptor.bin`

A raw binary dump of a USB descriptor tree, as it would appear in a
`OP_REP_IMPORT` USB/IP reply. The bytes are concatenated in standard USB
descriptor order:

1.  Device Descriptor (18 bytes, type 0x01)
2.  Configuration Descriptor (9 bytes, type 0x02)
3.  Interface Descriptor (9 bytes, type 0x04)
    - Optionally followed by class-specific descriptors (e.g. HID, type 0x21)
    - Followed by Endpoint Descriptors (7 bytes each, type 0x05)
4.  Additional configurations repeat step 2+

All multi-byte fields are little-endian (USB standard).

### `metadata.toml` Schema

Every field is optional. A missing field means "do not assert this property."

```toml
[device]
vendor_id = 0x046d          # u16: USB vendor ID (VID)
product_id = 0xc261         # u16: USB product ID (PID)
device_class = 3             # u8:  USB device class (0 = per-interface, 3 = HID)
device_sub_class = 0         # u8:  USB device sub-class
device_protocol = 0          # u8:  USB device protocol
max_packet_size0 = 64        # u8:  Max packet size for endpoint 0
num_configurations = 1       # u8:  Number of configurations

[config]
num_configs = 1              # usize: Number of configuration descriptors parsed

[interface]
class = 3                    # u8:  Interface class (3 = HID)
sub_class = 0                # u8:  Interface sub-class
protocol = 0                 # u8:  Interface protocol
num_endpoints = 1            # u8:  Number of endpoints on this interface

[[endpoints]]                # Array of endpoint expectations
address = 0x81               # u8:  Endpoint address (bit 7 = direction, bits 3-0 = number)
transfer_type = "interrupt"  # string: "control" | "isochronous" | "bulk" | "interrupt"
```

#### Endpoint `transfer_type` values

| TOML value     | `bm_attributes` bits 0-1 |
|----------------|--------------------------|
| `"control"`    | 0                        |
| `"isochronous"`| 1                        |
| `"bulk"`       | 2                        |
| `"interrupt"`  | 3                        |

## Adding a New Fixture

1.  Capture the descriptor tree bytes from a real device (see
    [CONTRIBUTING.md](../../../../CONTRIBUTING.md) for methods).
2.  Create a new subdirectory named after the device, e.g.
    `tests/fixtures/my_device/`.
3.  Place the raw descriptor bytes in `descriptor.bin`.
4.  Create `metadata.toml` with the expected properties.
5.  Run `cargo test -p usbip-core` to verify everything parses correctly.
