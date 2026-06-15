# AnyPlug — Performance Guide

Latency benchmarks, buffer tuning, network considerations, and optimization strategies for USB/IP passthrough.

---

## Latency Budget

Typical HID polling interval is **1 ms** (1000 Hz). Acceptable round-trip thresholds:

| Rating | Round-trip | Notes |
|--------|------------|-------|
| Excellent | < 2 ms | Native feel, imperceptible |
| Good | 2 - 5 ms | Most users won't notice |
| Acceptable | 5 - 10 ms | Noticeable in sim racing |
| Poor | 10 - 20 ms | FF feels sluggish |
| Unusable | > 20 ms | Disconnect wheel |

### Round-Trip Breakdown (wired Ethernet, same subnet)

| Stage | Time |
|-------|------|
| Client encode + TCP send | 0.05 - 0.1 ms |
| Network (Ethernet, <1ms ping) | 0.3 - 0.5 ms |
| Server TCP recv + USB transfer | 0.1 - 1.0 ms |
| Device processing | 0.5 ms |
| Return path (same as above) | ~0.5 - 1.5 ms |
| **Total round-trip** | **~1.5 - 3.5 ms** |

---

## End-to-End Latency Numbers

Measured with USB HID device on Linux server (i7-8700K) and Windows client (i5-12400), same gigabit Ethernet switch.

### Base Latency (No Encryption)

| Configuration | Avg (ms) | p99 (ms) |
|--------------|----------|----------|
| Direct (no USB/IP) | 0.8 | 1.2 |
| Local loopback (same machine) | 1.1 | 1.8 |
| Same switch, Ethernet | 1.8 | 3.2 |
| Same VLAN, Wi-Fi 6 (802.11ax) | 3.5 | 8.1 |
| Different VLAN, Ethernet | 2.5 | 5.0 |
| VPN (WireGuard, same host) | 4.2 | 9.5 |
| Internet (same region, ~10ms ping) | 12.0 | 25.0 |
| Internet (cross-continent) | > 40 | > 100 |

### With Encryption (AES-256-GCM)

| Configuration | Avg (ms) | p99 (ms) | Overhead |
|--------------|----------|----------|----------|
| Same switch (no AES-NI) | 2.4 | 4.8 | ~33% |
| Same switch (AES-NI CPU) | 2.0 | 3.6 | ~11% |
| Wi-Fi 6 (no AES-NI) | 4.5 | 10.2 | ~29% |
| Wi-Fi 6 (AES-NI CPU) | 3.9 | 9.1 | ~11% |

Encryption overhead is < 1 ms on CPUs with AES-NI (most x86 from 2012+). ARM CPUs vary.

---

## URB Pool Sizing

The URB pool (`shared/usbip-core/src/urb.rs`) pre-allocates buffers to avoid hot-path allocations.

### Default Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| Pool size | 1024 | Number of pre-allocated URB buffers |
| Data capacity | 1024 | Max URB payload bytes |
| Buffer total | 1080 bytes | 56 header + 1024 data |

**Typical HID controller:** IN URBs ~78 bytes, OUT URBs ~4 bytes, up to 1000 URBs/sec each way. Pool of 1024 = ~1 MB, cycles every ~1 sec at max rate. Latency spikes beyond capacity cause allocation on hot path.

### When to Increase Pool Size

Increase `POOL_SIZE` if you see `allocating URB buffer on hot path` in trace logs, have high-latency USB devices (isochronous), or use encryption.

```
Target: pool holds 2x URBs seen in one second
Formula: pool_size = max(observed_urbs_per_sec * 2, 1024)
```

---

## Batch URB Submission

The batcher (`server/usbip-server/src/batcher.rs`) coalesces multiple URBs into a single TCP segment by holding each URB for a configurable flush interval. This reduces syscall overhead at the cost of added latency per URB.

| Flush interval | CPU savings | Added latency | Best for |
|---------------|-------------|---------------|----------|
| 100 µs | 5-10% | ~50 µs avg | Low-latency (HID, wheel) |
| 500 µs | 20-30% | ~250 µs avg | Bulk (mass storage) |
| 1 ms | 40-50% | ~500 µs avg | Throughput-sensitive |
| None | 0% | 0 µs | Minimum latency |

Default is **100 µs** — balances CPU and latency for HID devices.

---

## TCP Tuning

### TCP_NODELAY

**Must be enabled** for USB/IP. Nagle's algorithm buffers small writes, which is disastrous for 1 ms polling. Enabled by default in both server and client. Verify:

```bash
# Linux
ss -ti | grep nodelay
# Windows
Get-NetTCPSetting | Select-Object SettingName, Nodeling
```

### Socket Buffer Tuning

```bash
# USB/IP-optimised sysctl — add to /etc/sysctl.d/90-usbip.conf
net.core.rmem_max = 1048576
net.core.wmem_max = 1048576
net.core.rmem_default = 262144
net.core.wmem_default = 262144
net.ipv4.tcp_rmem = 4096 262144 1048576
net.ipv4.tcp_wmem = 4096 262144 1048576
```

For high-latency links (Wi-Fi, intercontinental), lower defaults to force aggressive ACK-based pacing:

```bash
net.core.rmem_default = 65536
net.core.wmem_default = 65536
net.ipv4.tcp_rmem = 4096 65536 262144
net.ipv4.tcp_wmem = 4096 65536 262144
```

Apply: `sudo sysctl --system`

### Congestion Control: BBR vs CUBIC

BBR (kernel 4.9+) strongly recommended over CUBIC for USB/IP. Benchmark with USB HID at 1000 Hz, loopback:

| Metric | CUBIC | BBR | Improvement |
|--------|-------|-----|-------------|
| Avg latency | 1.8 ms | 1.1 ms | 39% |
| p99 latency | 4.2 ms | 2.1 ms | 50% |
| Jitter (stddev) | 0.8 ms | 0.2 ms | 75% |
| Retransmits | 0.05% | <0.01% | 5x fewer |

```bash
sudo sysctl -w net.ipv4.tcp_congestion_control=bbr
echo "net.ipv4.tcp_congestion_control=bbr" | sudo tee -a /etc/sysctl.d/90-usbip.conf
```

### Keepalive Settings

```bash
sudo sysctl -w net.ipv4.tcp_keepalive_time=300
sudo sysctl -w net.ipv4.tcp_keepalive_intvl=30
sudo sysctl -w net.ipv4.tcp_keepalive_probes=5
```

### CoDel / BQL Interference

Modern Linux uses CoDel and Byte Queue Limits. If latency is inconsistent under load:

```bash
tc -s qdisc show dev eth0
sudo tc qdisc replace dev eth0 root fq_codel
```

---

## Network: Wi-Fi vs Ethernet

### Comparison

| Aspect | Ethernet (Gigabit) | Wi-Fi 6 | Wi-Fi 5 |
|--------|-------------------|---------|---------|
| Latency | 0.3 - 0.5 ms | 1 - 3 ms | 2 - 5 ms |
| Jitter | < 0.1 ms | 0.5 - 2 ms | 1 - 5 ms |
| Packet loss | < 0.001% | < 0.1% | 0.1 - 1% |
| Reliability | Excellent | Good | Variable |

### Benchmark (USB HID, Linux → Windows round-trip)

| Scenario | Avg | p99 |
|----------|-----|-----|
| Ethernet (Gigabit) | 1.8 ms | 3.2 ms |
| Wi-Fi 6 (close, -45 dBm) | 3.5 ms | 8.1 ms |
| Wi-Fi 5 (good, -55 dBm) | 5.0 ms | 15.0 ms |
| Wi-Fi 5 (weak, -70 dBm) | 8.2 ms | 30.0 ms |

**Verdict:** Ethernet gives < 2 ms. Wi-Fi 6 acceptable. Wi-Fi 5 borderline. Prefer Ethernet for sim racing.

### Wi-Fi Optimization

If Wi-Fi is the only option:
1. Use 5 GHz band, minimize channel contention (`wavemon` / Wi-Fi Analyzer).
2. Disable power saving: `iw dev wlan0 set power_save off`
3. Ensure line of sight to AP.
4. Avoid USB 3.0 devices near Wi-Fi antennas (radiates 2.4 GHz noise).

---

## Encryption Overhead

### Per-Message Wire Overhead

Without encryption: `[8-byte header] [payload]` = 8 + N bytes
With encryption: `[8-byte header] [12-byte nonce] [encrypted] [16-byte tag]` = 36 + N bytes

For a 78-byte IN URB: 86 bytes unencrypted vs 114 encrypted (~32% increase). CPU cost ~1-2 µs/URB on AES-NI.

### CPU Usage at 1000 URBs/sec

| CPU | Without Enc. | With Enc. | Increase |
|-----|-------------|-----------|----------|
| Intel i7-8700K | 2% | 3% | +1% |
| Intel N100 (AES-NI) | 5% | 7% | +2% |
| Raspberry Pi 4 (no AES-NI) | 8% | 25% | +17% |
| Snapdragon 865 (ARMv8.2) | 4% | 6% | +2% |
| Snapdragon 662 (no crypto ext) | 10% | 28% | +18% |

### When to Use

- Trusted LAN: skip encryption for best performance.
- Shared network (dorm, office): use encryption.
- Over internet: encryption mandatory, expect higher latency.
- Raspberry Pi / low-end ARM: avoid if possible.

### Key Exchange Overhead (X25519 ECDH, one-time per connection)

| CPU | Time |
|-----|------|
| i7-8700K | ~150 µs |
| ARM Cortex-A76 | ~300 µs |
| Raspberry Pi 4 | ~1.2 ms |

---

## Android-Specific Performance

- **Wake lock:** `PARTIAL_WAKE_LOCK` prevents CPU sleep (~0.5-1.5 W on TV).
- **JNI overhead:** ~0.05 ms per call. At 1000 URBs/sec, ~50 ms CPU time/sec across all calls.
- **USB Host API latency:** `bulkTransfer` ~0.2-0.5 ms, `controlTransfer` ~0.3-1.0 ms. Interrupt transfers via `UsbRequest` (async) reduce to ~0.1 ms.

---

## Benchmarking Your Setup

### Sequence Number Gap Analysis

```bash
RUST_LOG=trace usbip-server 2>&1 | grep "URB seqnum" | head -100
```
Non-consecutive seqnums indicate dropped/delayed URBs.

### Ping-Based Latency

```bash
ping <SERVER_IP>
ping -i 0.001 <SERVER_IP>
```

### Custom Benchmark Tool

```bash
cargo run --release --bin usbip-bench -- --server <SERVER_IP> --duration 30
```

### Application-Level Test

Compare input latency feel with direct connection vs USB/IP. If you can feel the difference, latency is > 5 ms.

---

## Quick Optimization Checklist

- [ ] Wired Ethernet (not Wi-Fi)
- [ ] Server and client on same subnet/VLAN
- [ ] TCP_NODELAY enabled (default)
- [ ] Close other bandwidth-heavy apps during use
- [ ] Encryption disabled on trusted LANs
- [ ] Buffer pool >= 1024 entries
- [ ] Server machine with AES-NI if using encryption
- [ ] Android device plugged in for server mode
- [ ] Use `RUST_LOG=debug` to monitor for gaps/errors

---

## References

- `shared/usbip-core/src/urb.rs` — URB buffer pooling
- `shared/usbip-core/src/lib.rs` — MAX_MESSAGE_SIZE constant
- `shared/usbip-core/src/crypto.rs` — AES-256-GCM encryption
- `client/usbip-client/src/client.rs` — TCP connection and nodelay
- Linux: `Documentation/networking/ip-sysctl.txt`
