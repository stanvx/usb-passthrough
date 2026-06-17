# Port Model

AnyPlug uses three TCP/UDP ports. Each serves a distinct purpose and should be
configured independently.

| Port | Name | Protocol | Default | Purpose |
|------|------|----------|---------|---------|
| 3240 | Wire port | TCP | 3240 | USB/IP protocol traffic |
| 3241 | API port | TCP | 3241 | REST API + WebSocket |
| 5353 | mDNS port | UDP | 5353 | Service discovery |

---

## Wire Port (3240)

**What flows here:** Raw USB/IP protocol packets — USB request blocks (URBs),
device descriptors, and isochronous data. This is the core passthrough data
path. Every URB round-trip (ioctl → TCP send → network → TCP recv → device)
traverses this port.

**Who listens:** `usbip-server` binds this port. A single TCP connection per
imported device.

**CLI flag:** `--port` / `-p`

```
usbip-server --port 3240
```

**Config key:** `port` in `server.toml` / `PUT /api/config`

**Firewall:**
- Linux: `sudo ufw allow 3240/tcp`
- Windows (Admin PowerShell):
  ```powershell
  New-NetFirewallRule -DisplayName "AnyPlug Wire" -Direction Inbound -Protocol TCP -LocalPort 3240 -Action Allow
  ```

---

## API Port (3241)

**What flows here:** REST API calls (status, devices, config, scan, connect,
disconnect) and WebSocket events (latency telemetry, connection state). Used
by the web console, health checks, and automation scripts.

**Who listens:** `usbip-server` binds this port only when `--api-port` is set.

**CLI flag:** `--api-port`

```
usbip-server --api-port 3241
```

**Config key:** `api_port` in `server.toml` / `PUT /api/config`

**Endpoints:**
- `GET  /api/status` — server health and metrics
- `GET  /api/devices` — list exportable USB devices
- `GET  /api/config` — read server configuration
- `PUT  /api/config` — update server configuration
- `POST /api/connect` — import a device from a remote server
- `POST /api/disconnect` — release an imported device
- `POST /api/scan` — discover USB/IP servers on the LAN
- `WS   /api/events` — real-time event stream (latency, connect/disconnect)

**Firewall:**
- Linux: `sudo ufw allow 3241/tcp`
- Windows (Admin PowerShell):
  ```powershell
  New-NetFirewallRule -DisplayName "AnyPlug API" -Direction Inbound -Protocol TCP -LocalPort 3241 -Action Allow
  ```

---

## mDNS Port (5353)

**What flows here:** UDP multicast packets advertising `_usbip._tcp.local`
services. Lets clients discover servers without manual IP configuration.

**Who listens:** `usbip-server` broadcasts via Avahi (Linux) or native mDNS
(macOS / Windows) on this port. The server announces itself as
`_usbip._tcp.local` with TXT records containing device info.

**Note:** mDNS is link-local — it does not cross VLANs, subnets, or
enterprise Wi-Fi access points. For cross-subnet operation, use a direct IP
connection (wire port) or configure an mDNS gateway/reflector.

**Firewall:**
- Linux: `sudo ufw allow 5353/udp`
- Windows (Admin PowerShell):
  ```powershell
  New-NetFirewallRule -DisplayName "AnyPlug mDNS" -Direction Inbound -Protocol UDP -LocalPort 5353 -Action Allow
  ```

---

## Quick Reference

| Scenario | Ports needed |
|----------|-------------|
| Direct IP connection | Wire (3240) |
| API + web console | Wire (3240), API (3241) |
| mDNS discovery | Wire (3240), mDNS (5353) |
| Full setup | Wire (3240), API (3241), mDNS (5353) |
