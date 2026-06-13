# Hot-plug detection: push updated device list to all connected clients

USB device attach/detach events after server start must be detected and communicated to all connected clients, per ADR-0003 and the v1.0 reliability ordering. The server registers platform-specific OS callbacks, diffs the device list on each event, and broadcasts an updated `OP_REP_DEVLIST` to every connected client. Clients do not poll.

## Platform seams

Each platform detects USB events through its native mechanism, but all three produce the same `HotplugEvent` struct with a fresh `CorrelationId`, diffed busids, and a full device-list snapshot.

**Linux:** `rusb::HotplugBuilder` with `LIBUSB_HOTPLUG_EVENT_DEVICE_ARRIVED` / `LIBUSB_HOTPLUG_EVENT_DEVICE_LEFT`. The callback fires on a libusb-internal thread and uses `try_send` (non-blocking) to push into the broadcast channel. The `HotplugBuilder` is registered in `UsbDeviceManager::new()`.

**Windows:** `RegisterDeviceNotification` with `DBT_DEVTYP_DEVICEINTERFACE` on `GUID_DEVINTERFACE_USB_DEVICE`. The Windows Service message pump receives `WM_DEVICECHANGE` and posts the event.

**Android:** `UsbManager` broadcasts `ACTION_USB_DEVICE_ATTACHED` / `ACTION_USB_DEVICE_DETACHED`. The existing JNI bridge (`RustBridge.kt`) already receives these for manual enumeration; we add a JNI callback `on_hotplug_event(is_attach: Boolean, device_name: String)` that the Rust side listens on.

## Protocol

No new command code, no new wire format. The server reuses `OP_REP_DEVLIST` (header + ndev + entries) and sends it unsolicited when the device list changes. The client's protocol handler already parses `OP_REP_DEVLIST`; it must now handle receiving one outside the request/reply cycle.

**Server behavior:** Every connected client receives the updated list on each hotplug event, regardless of whether that client has an active import. Each client filters to its own interests.

**Client behavior:** In discovery mode, update the displayed device list. With an active import, if the imported device's busid appears in the `removed` set, the next URB will return `STATUS_ST_NODEV` and the error infrastructure (#8) classifies it as `ErrorCategory::Permanent` — the auto-reconnect layer (#15) handles it from there. New devices from the same server require no client action.

## Data flow

```
OS hotplug callback (any thread)
  └─► UsbDeviceManager::on_hotplug_event()
        ├─ enumerate current devices (Context::devices())
        ├─ diff against previous snapshot
        ├─ construct HotplugEvent {
        │     added: Vec<String>,
        │     removed: Vec<String>,
        │     devices: Vec<UsbIpDeviceEntry>,
        │     correlation_id: CorrelationId,
        │   }
        ├─ store new snapshot
        └─ try_send to broadcast::Sender<HotplugEvent>

broadcast channel
  └─► Server::run() accept loop
        ├─ each client task holds a broadcast::Receiver<HotplugEvent>
        └─ spawns a forwarder that writes OP_REP_DEVLIST on each event
```

## Error handling

- Every `HotplugEvent` carries a `CorrelationId` (UUIDv7), consistent with #8.
- If the broadcast channel is full (slow consumer), the oldest event is dropped and a warning is logged with the dropped event's correlation_id.
- If device enumeration fails during a hotplug callback, the callback emits a `HotplugEvent` with an empty `devices` vec and sets an `error` field rather than returning stale data. Clients see a transient empty list.

## Testing strategy

- **Unit tests:** `HotplugEvent` construction, correlation_id assignment, device-list diffing (added/removed/unmodified detection), empty-to-populated and populated-to-empty transitions.
- **Integration tests:** The QEMU test rig (configfs + dummy_hcd/udc) can exercise Linux hotplug by binding/unbinding the configfs gadget while the server is running. The test asserts that the server emits an `OP_REP_DEVLIST` on each change with correct added/removed busids.
- **Stub for CI:** A `#[cfg(test)]` hotplug callback path that fires synthetic events so unit tests can verify the broadcast channel and client-side handling without a physical USB bus.

## Considered options

- **Auto-claim on hotplug:** Rejected. Couples detection to access-control policy. The server's `require_confirmation` and `allowed_vid_pid` flags already handle import authorization at connect time. Hotplug should report presence, not pre-approve exports.
- **No push, clients poll with OP_REQ_DEVLIST:** Rejected. Adds latency proportional to poll interval and wastes network bandwidth. A 2-second poll window misses sub-second disconnect/reconnect cycles.
- **New protocol command for deltas:** Rejected. `OP_REP_DEVLIST` already carries the full device list. Deltas (added/removed busids) are metadata attached to the event, not the wire message.

## Consequences

- The server's thread model gains a hotplug callback registration in `UsbDeviceManager::new()` and a per-client broadcast forwarder in the accept loop.
- The client's protocol handler must accept unsolicited `OP_REP_DEVLIST` messages. This is a backward-compatible change — the parser is unchanged, only the dispatch rule changes.
- Hotplug events are observable facts with correlation IDs, making them composable with #14 (REST API + WebSocket events) and diagnosable in logs.
- Windows and Android hotplug are architectural sketches at this stage; only Linux is implemented and tested in CI. The platform seams are `#[cfg(target_os)]` branches in `UsbDeviceManager`, not a trait abstraction.
