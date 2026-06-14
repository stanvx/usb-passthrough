# AnyPlug Server — Home Assistant Add-on

USB/IP passthrough server add-on for Home Assistant. Exports locally connected USB devices over the network to AnyPlug clients, allowing them to appear as if directly attached.

## How it works

The add-on runs the `usbip-server` binary inside an Alpine container with full USB access. Once started, it listens for AnyPlug client connections and exports discoverable USB devices from the host.

## Prerequisites

- Home Assistant OS or Supervised installation
- USB devices plugged into the host (or passed through to the VM)
- AnyPlug client on target machine (Linux, Windows, or Android)

## Installation

1. Add this repository to your Home Assistant add-on store
2. Install the **AnyPlug Server** add-on
3. Configure the options below
4. Start the add-on

## Configuration

| Option          | Type      | Default | Description                                    |
|-----------------|-----------|---------|------------------------------------------------|
| `port`          | integer   | `3240`  | TCP port for USB/IP server connections         |
| `metrics_port`  | integer   | `0`     | API / metrics endpoint port (`0` = disabled)   |
| `encryption`    | boolean   | `false` | Enable AES-256-GCM encryption                  |
| `allowlist`     | list[str] | `[]`    | Restrict to specific VID:PID pairs (empty = all) |

### Port

The main USB/IP protocol port. Must match the port configured on AnyPlug clients. Default: `3240`.

### Metrics port

Reserved for a future release that will expose a REST API with health checks and connection metrics on this port. Set to `0` (default) to disable.

### Encryption

When enabled, all data between server and client is encrypted using AES-256-GCM with X25519 ECDH key exchange. Both sides must enable encryption.

### Allowlist

List of `VID:PID` hex pairs to restrict which USB devices are exported. Example:

```yaml
allowlist:
  - "046d:c261"   # Logitech G920
  - "03f0:1234"   # HP printer
```

When empty (default), all connected USB devices are advertised to clients.

## Network

The add-on requires `host_network: true` to discover USB devices attached to the host and to make mDNS/broadcast discovery work reliably. The configured port is exposed on the host network.

## Security

- Devices are served read-write — connected clients have full USB access to exported devices
- Use `allowlist` to restrict which devices are visible
- Enable `encryption` for untrusted networks
- Consider running only when needed — stop the add-on when not in use

## Troubleshooting

**No devices shown on client:**
- Verify USB device is physically connected to the HA host
- Check add-on logs for `[ANYPLUG]` messages
- Run `usbip list -r <ha-host>` from the client

**Permission errors:**
- Ensure add-on has `full_access: true` and `privileged` flags enabled
- On some systems, USB device nodes need `SYS_USBDEV` capability

## Version history

See `CHANGELOG.md`.

## References

- [AnyPlug project](https://github.com/stanvx/AnyPlug)
- [USB/IP protocol](https://www.kernel.org/doc/html/latest/usb/usbip_protocol.html)
- [HA add-on development](https://developers.home-assistant.io/docs/add-ons/)
