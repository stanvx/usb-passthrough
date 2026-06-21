//! USB/IP Client — Import USB devices over TCP.
//!
//! ## Architecture
//!
//! ```text
//! main()
//!   ├─ mDNS discovery thread: browses _usbip._tcp.local
//!   ├─ TCP connect → server
//!   │    ├─ send OP_REQ_DEVLIST → receive OP_REP_DEVLIST
//!   │    ├─ send OP_REQ_IMPORT  → receive OP_REP_IMPORT (+ descriptors)
//!   │    └─ VHCI thread: forwards kernel URBs ↔ server
//!   └─ VHCI event thread: kernel URB completion callbacks
//! ```
//!
//! ## Reconnect
//!
//! When the TCP connection drops (transient error), the client enters
//! exponential backoff: 1s → 2s → 4s → 8s → 16s → 30s (capped).
//! On each retry it re-establishes the TCP connection, re-imports the
//! device from the server, and re-attaches the VHCI device. Permanent
//! failures (auth rejected, device not found) stop immediately.

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tracing::{error, info, info_span, warn};
use uuid::Uuid;

use zerocopy::FromBytes;
use zerocopy::IntoBytes;

use usbip_core::descriptor::*;
use usbip_core::error::*;
use usbip_core::protocol::*;
use usbip_core::urb::*;

use crate::discovery::MdnsBrowser;
use crate::reconnect::{ReconnectConfig, ReconnectDecision, ReconnectState};
use crate::vhci::{VhciBackend, VhciDriver};

/// Client configuration.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Server address (if manual connection).
    pub server_addr: Option<SocketAddr>,
    /// Use mDNS for discovery.
    pub use_mdns: bool,
    /// Reconnect on transient disconnect.
    pub auto_reconnect: bool,
    /// Reconnect configuration (backoff, retry limits).
    pub reconnect: ReconnectConfig,
    /// TCP nodelay.
    pub tcp_nodelay: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_addr: None,
            use_mdns: true,
            auto_reconnect: true,
            reconnect: ReconnectConfig::default(),
            tcp_nodelay: true,
        }
    }
}

/// USB/IP client.
pub struct Client {
    config: ClientConfig,
    vhci: Arc<dyn VhciBackend>,
    /// Active connections: busid -> (stream, device_info).
    connections: Mutex<Vec<ActiveConnection>>,
}

#[allow(dead_code)]
struct ActiveConnection {
    busid: String,
    device_entry: UsbIpDeviceEntry,
    descriptors: Vec<u8>,
}

impl Client {
    pub fn new(config: ClientConfig) -> UsbIpResult<Self> {
        let vhci = VhciDriver::new()?;
        Self::new_with_vhci(config, Arc::new(vhci))
    }

    /// Construct a client with an injected VHCI backend (test seam).
    pub(crate) fn new_with_vhci(
        config: ClientConfig,
        vhci: Arc<dyn VhciBackend>,
    ) -> UsbIpResult<Self> {
        Ok(Self { config, vhci, connections: Mutex::new(Vec::new()) })
    }

    /// Discover servers via mDNS.
    pub fn discover_servers(&self) -> UsbIpResult<Vec<SocketAddr>> {
        let browser = MdnsBrowser::new()?;
        browser.browse()
    }

    /// Connect to a server and list its devices.
    pub async fn list_remote_devices(
        &self,
        addr: SocketAddr,
    ) -> UsbIpResult<Vec<UsbIpDeviceEntry>> {
        let mut stream = TcpStream::connect(addr).await?;
        if self.config.tcp_nodelay {
            stream.set_nodelay(true)?;
        }

        // Send OP_REQ_DEVLIST
        let header = UsbIpHeader::new(OP_REQ_DEVLIST);
        stream.write_all(header.as_bytes()).await?;

        // Read reply header
        let mut header_buf = [0u8; 8];
        stream.read_exact(&mut header_buf).await?;

        let rep_header = match UsbIpHeader::read_from_prefix(&header_buf) {
            Ok((h, _)) => h,
            Err(_) => {
                return Err(UsbIpError::from(ErrorKind::Protocol("invalid reply header".into())))
            },
        };

        if rep_header.command.get() != OP_REP_DEVLIST {
            return Err(UsbIpError::from(ErrorKind::Protocol("unexpected reply".into())));
        }

        // Read ndev
        let mut ndev_buf = [0u8; 4];
        stream.read_exact(&mut ndev_buf).await?;
        let ndev = u32::from_be_bytes(ndev_buf);

        // Read device entries
        let mut devices = Vec::with_capacity(ndev as usize);
        for _ in 0..ndev {
            let mut entry_buf = vec![0u8; UsbIpDeviceEntry::SIZE];
            stream.read_exact(&mut entry_buf).await?;
            if let Ok((entry, _)) = UsbIpDeviceEntry::read_from_prefix(&entry_buf) {
                devices.push(entry);
            }
        }

        Ok(devices)
    }

    /// Import a USB device from a server, with optional auto-reconnect.
    ///
    /// This creates a virtual USB device on the local system via VHCI.
    /// If `auto_reconnect` is enabled in config, the URB forwarding loop
    /// will automatically retry on transient network failures using
    /// exponential backoff. Permanent errors (auth rejected, device not
    /// found) and fatal errors stop immediately.
    ///
    /// The returned handle lets you monitor/detach the device. The URB
    /// forwarding runs in a background task and will attempt reconnection
    /// until the retry limit is exhausted or a non-transient error occurs.
    pub async fn import_device(
        &self,
        addr: SocketAddr,
        busid: &str,
    ) -> UsbIpResult<ImportedDevice> {
        let correlation_id = Uuid::now_v7();
        let span = info_span!("import_device", correlation_id = %correlation_id, busid = %busid, server = %addr);
        let _guard = span.enter();

        let vhci = self.vhci.clone();
        let config = self.config.clone();
        let busid_owned = busid.to_string();
        let addr_owned = addr;

        // Do the initial import (connect + VHCI attach)
        let (device_entry, descriptors) =
            do_import(addr_owned, &busid_owned, &*vhci, config.tcp_nodelay).await?;

        // Store connection info
        {
            let mut conns = self.connections.lock().await;
            conns.push(ActiveConnection {
                busid: busid_owned.clone(),
                device_entry: device_entry.clone(),
                descriptors: descriptors.clone(),
            });
        }

        info!("Imported device: {} ({}:{})", busid, addr, busid);

        // Spawn URB forwarding with auto-reconnect
        tokio::spawn(async move {
            let mut state = ReconnectState::Active;
            let reconf = config.reconnect.clone();

            loop {
                // Run the URB forwarding loop (or the initial one)
                let result =
                    urb_forwarding_task(addr_owned, &busid_owned, vhci.clone(), config.tcp_nodelay)
                        .await;

                match result {
                    Ok(()) => {
                        // Clean shutdown (e.g. Ctrl+C) — stop retrying
                        info!("URB loop ended cleanly for {}", busid_owned);
                        break;
                    },
                    Err(ref e) => {
                        if !config.auto_reconnect {
                            error!("URB forwarding error (auto-reconnect disabled): {}", e);
                            break;
                        }

                        match crate::reconnect::decide_reconnect(&result, &mut state, &reconf) {
                            ReconnectDecision::RetryAfter(delay) => {
                                warn!(
                                    busid = %busid_owned,
                                    server = %addr_owned,
                                    delay_ms = delay.as_millis(),
                                    error = %e,
                                    "URB loop failed, reconnecting...",
                                );
                                tokio::time::sleep(delay).await;
                                // Continue the loop to re-connect + re-import
                            },
                            ReconnectDecision::Stop => {
                                // Permanent/fatal error or retries exhausted
                                error!(
                                    busid = %busid_owned,
                                    server = %addr_owned,
                                    error = %e,
                                    "Permanent error or retries exhausted, stopping",
                                );
                                break;
                            },
                        }
                    },
                }
            }

            info!("Reconnect loop ended for {}", busid_owned);
        });

        Ok(ImportedDevice { busid: busid.to_string(), device_entry })
    }

    /// Single-shot import — connect, import, attach VHCI. No background task.
    ///
    /// Unlike [`import_device`], this does NOT spawn a reconnect loop. The caller
    /// is responsible for managing reconnection (the daemon uses this so it can
    /// await the URB forwarding loop directly).
    pub async fn import_device_once(
        &self,
        addr: SocketAddr,
        busid: &str,
    ) -> UsbIpResult<ImportedDevice> {
        let (device_entry, descriptors) =
            do_import(addr, busid, &*self.vhci, self.config.tcp_nodelay).await?;

        {
            let mut conns = self.connections.lock().await;
            conns.push(ActiveConnection {
                busid: busid.to_string(),
                device_entry: device_entry.clone(),
                descriptors,
            });
        }

        Ok(ImportedDevice { busid: busid.to_string(), device_entry })
    }

    /// Run the URB forwarding loop for a previously-imported device.
    ///
    /// Opens a fresh TCP connection, re-imports, and enters the URB forwarding
    /// loop.  Returns `Ok(())` on clean shutdown or `Err` on disconnect/error.
    /// Callers (like the daemon) `tokio::select!` on this to detect drops
    /// without polling.
    pub async fn run_urb_forwarding(&self, addr: SocketAddr, busid: &str) -> UsbIpResult<()> {
        urb_forwarding_task(addr, busid, self.vhci.clone(), self.config.tcp_nodelay).await
    }
}

/// Handle for an imported device.
#[derive(Debug, Clone)]
pub struct ImportedDevice {
    pub busid: String,
    pub device_entry: UsbIpDeviceEntry,
}

/// One-shot import: TCP connect → OP_REQ_IMPORT → VHCI attach.
///
/// Returns the device entry and descriptor tree on success.
async fn do_import(
    addr: SocketAddr,
    busid: &str,
    vhci: &dyn VhciBackend,
    tcp_nodelay: bool,
) -> UsbIpResult<(UsbIpDeviceEntry, Vec<u8>)> {
    let mut stream = TcpStream::connect(addr).await?;
    if tcp_nodelay {
        stream.set_nodelay(true)?;
    }

    // Send OP_REQ_IMPORT with busid
    let header = UsbIpHeader::new(OP_REQ_IMPORT);
    stream.write_all(header.as_bytes()).await?;

    let mut busid_buf = [0u8; 32];
    let busid_bytes = busid.as_bytes();
    let copy_len = busid_bytes.len().min(31);
    busid_buf[..copy_len].copy_from_slice(&busid_bytes[..copy_len]);
    stream.write_all(&busid_buf).await?;

    // Read OP_REP_IMPORT
    let mut header_buf = [0u8; 8];
    stream.read_exact(&mut header_buf).await?;

    let rep_header = match UsbIpHeader::read_from_prefix(&header_buf) {
        Ok((h, _)) => h,
        Err(_) => return Err(UsbIpError::from(ErrorKind::Protocol("invalid import reply".into()))),
    };

    if rep_header.command.get() != OP_REP_IMPORT {
        return Err(UsbIpError::from(ErrorKind::Protocol("unexpected reply".into())));
    }

    if rep_header.status.get() != STATUS_SUCCESS {
        return Err(UsbIpError::from(ErrorKind::DeviceBusy(format!(
            "server rejected import (status={})",
            rep_header.status.get()
        ))));
    }

    // Read device entry
    let mut entry_buf = vec![0u8; UsbIpDeviceEntry::SIZE];
    stream.read_exact(&mut entry_buf).await?;
    let device_entry = match UsbIpDeviceEntry::read_from_prefix(&entry_buf) {
        Ok((entry, _)) => entry,
        Err(_) => return Err(UsbIpError::from(ErrorKind::Protocol("invalid device entry".into()))),
    };

    // Read descriptor tree until fully parsed
    let mut descriptors = Vec::new();
    let tree_estimate = 512;
    let mut desc_buf = vec![0u8; tree_estimate];
    loop {
        match stream.read(&mut desc_buf[descriptors.len()..]).await {
            Ok(0) => break,
            Ok(n) => {
                let prev_len = descriptors.len();
                descriptors.extend_from_slice(&desc_buf[prev_len..prev_len + n]);
                if UsbDeviceInfo::parse_descriptor_tree(&descriptors).is_some() {
                    break;
                }
            },
            Err(_) => break,
        }
    }

    // Create virtual device via VHCI
    let _vhci_device = vhci.create_device(&device_entry, &descriptors)?;

    Ok((device_entry, descriptors))
}

/// Full import + URB loop, intended for the reconnect loop.
///
/// Each call: TCP connect → OP_REQ_IMPORT → VHCI attach → URB loop.
/// Returns Ok(()) on clean shutdown, Err on failure.
async fn urb_forwarding_task(
    addr: SocketAddr,
    busid: &str,
    vhci: Arc<dyn VhciBackend>,
    tcp_nodelay: bool,
) -> UsbIpResult<()> {
    let mut stream = tcp_connect_and_import(addr, busid, tcp_nodelay).await?;

    let correlation_id = Uuid::now_v7();
    let span = info_span!("urb_forwarding", correlation_id = %correlation_id, busid = %busid, server = %addr);
    let _guard = span.enter();

    let result = urb_forwarding_loop(&mut stream, &*vhci).await;
    info!("URB forwarding loop ended: {:?}", result.as_ref().err());
    result
}

/// TCP connect + OP_REQ_IMPORT handshake.
async fn tcp_connect_and_import(
    addr: SocketAddr,
    busid: &str,
    tcp_nodelay: bool,
) -> UsbIpResult<TcpStream> {
    let mut stream = TcpStream::connect(addr).await?;
    if tcp_nodelay {
        stream.set_nodelay(true)?;
    }

    // Send OP_REQ_IMPORT with busid
    let header = UsbIpHeader::new(OP_REQ_IMPORT);
    stream.write_all(header.as_bytes()).await?;

    let mut busid_buf = [0u8; 32];
    let busid_bytes = busid.as_bytes();
    let copy_len = busid_bytes.len().min(31);
    busid_buf[..copy_len].copy_from_slice(&busid_bytes[..copy_len]);
    stream.write_all(&busid_buf).await?;

    // Read OP_REP_IMPORT
    let mut header_buf = [0u8; 8];
    stream.read_exact(&mut header_buf).await?;

    let rep_header = match UsbIpHeader::read_from_prefix(&header_buf) {
        Ok((h, _)) => h,
        Err(_) => return Err(UsbIpError::from(ErrorKind::Protocol("invalid import reply".into()))),
    };

    if rep_header.command.get() != OP_REP_IMPORT {
        return Err(UsbIpError::from(ErrorKind::Protocol("unexpected reply".into())));
    }

    if rep_header.status.get() != STATUS_SUCCESS {
        return Err(UsbIpError::from(ErrorKind::DeviceBusy(format!(
            "server rejected import (status={})",
            rep_header.status.get()
        ))));
    }

    // Read device entry (validate but don't attach VHCI here)
    let mut entry_buf = vec![0u8; UsbIpDeviceEntry::SIZE];
    stream.read_exact(&mut entry_buf).await?;

    // Read descriptor tree anchor bytes.
    // Read a small chunk so we do not slurp the next message (e.g. RET_SUBMIT)
    // that the server may have already queued on the wire.
    let mut desc_buf = [0u8; 18];
    if let Ok(n) = stream.read(&mut desc_buf).await {
        if n == 0 {
            return Err(UsbIpError::from(ErrorKind::ConnectionClosed));
        }
    }

    Ok(stream)
}

/// Main URB forwarding loop — bidirectional proxy between VHCI and server.
async fn urb_forwarding_loop(stream: &mut TcpStream, vhci: &dyn VhciBackend) -> UsbIpResult<()> {
    let mut header_buf = [0u8; 8];

    loop {
        // Read from server
        tokio::select! {
            result = stream.read_exact(&mut header_buf) => {
                if result.is_err() {
                    break; // server disconnected
                }
            }
            _ = tokio::signal::ctrl_c() => {
                break;
            }
        }

        let header = match UsbIpHeader::read_from_prefix(&header_buf) {
            Ok((h, _)) => h,
            Err(_) => break,
        };

        match header.command.get() {
            USBIP_CMD_SUBMIT => {
                // Server -> Client CMD_SUBMIT is unusual but possible
                // in symmetric USB/IP implementations
            },
            USBIP_RET_SUBMIT => {
                // Read RET_SUBMIT
                let mut ret_buf = vec![0u8; UsbIpRetSubmit::HEADER_SIZE];
                stream.read_exact(&mut ret_buf).await?;

                let ret = match UsbIpRetSubmit::read_from_prefix(&ret_buf) {
                    Ok((r, _)) => r,
                    Err(_) => {
                        return Err(UsbIpError::from(ErrorKind::Protocol(
                            "invalid RET_SUBMIT".into(),
                        )))
                    },
                };

                // Read data if IN transfer
                let mut in_data = Vec::new();
                if ret.has_data() {
                    let data_len = ret.actual_len() as usize;
                    let mut data = vec![0u8; data_len];
                    stream.read_exact(&mut data).await?;
                    in_data = data;
                }

                // Complete the URB on the VHCI side
                vhci.complete_urb(
                    ret.seqnum(),
                    ret.devid(),
                    ret.status_val() as i32,
                    ret.actual_len(),
                    &in_data,
                )?;
            },
            USBIP_RET_UNLINK => {
                let mut unlink_buf = vec![0u8; UsbIpRetUnlink::SIZE];
                stream.read_exact(&mut unlink_buf).await?;

                let unlink = match UsbIpRetUnlink::read_from_prefix(&unlink_buf) {
                    Ok((u, _)) => u,
                    Err(_) => {
                        return Err(UsbIpError::from(ErrorKind::Protocol(
                            "invalid RET_UNLINK".into(),
                        )))
                    },
                };

                vhci.cancel_urb(unlink.seqnum(), unlink.devid())?;
            },
            _ => {
                warn!("Unknown command in URB loop: 0x{:04x}", header.command.get());
            },
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    //! RED tests for issue #28: `Client::new_with_vhci` injection seam.
    //!
    //! These tests assert the contract that the GREEN step must satisfy:
    //! a `Client` constructed via `new_with_vhci` must route its VHCI
    //! operations to the supplied backend, not to the platform default.
    //! The tests drive the client through a real `TcpListener` playing
    //! the role of a USB/IP server, send a crafted `USBIP_RET_SUBMIT`,
    //! and observe the result on `MockVhciBackend`.

    use super::*;

    use std::sync::Arc as StdArc;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use usbip_core::protocol::{
        UsbIpHeader, OP_REP_IMPORT, OP_REQ_IMPORT, STATUS_SUCCESS, URB_DIR_IN, USBIP_RET_SUBMIT,
    };
    use usbip_core::urb::UsbIpRetSubmit;

    use crate::vhci::MockVhciBackend;
    use crate::vhci::VhciBackend;

    /// Build a minimal 312-byte `UsbIpDeviceEntry` for the test server.
    fn make_device_entry_bytes(busid: &str) -> Vec<u8> {
        let mut entry = vec![0u8; 312];
        let busid_bytes = busid.as_bytes();
        let copy_len = busid_bytes.len().min(31);
        entry[0x100..0x100 + copy_len].copy_from_slice(&busid_bytes[..copy_len]);
        entry
    }

    /// Minimal end-to-end RED: construct via `new_with_vhci`, drive a
    /// single `USBIP_RET_SUBMIT` through `run_urb_forwarding`, and
    /// confirm the injected mock recorded the completion.
    #[tokio::test]
    async fn test_new_with_vhci_routes_complete_urb_to_injected_backend() {
        let mock = StdArc::new(MockVhciBackend::new());
        let backend: StdArc<dyn VhciBackend> = mock.clone();

        // Bind a localhost listener that will act as the USB/IP server.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Server task: accept, complete the OP_REQ_IMPORT handshake,
        // then send one USBIP_RET_SUBMIT, then close.
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();

            // Read the 8-byte OP_REQ_IMPORT header.
            let mut hdr = [0u8; 8];
            sock.read_exact(&mut hdr).await.unwrap();
            let cmd = u16::from_be_bytes([hdr[2], hdr[3]]);
            assert_eq!(cmd, OP_REQ_IMPORT);

            // Read the 32-byte busid.
            let mut busid = [0u8; 32];
            sock.read_exact(&mut busid).await.unwrap();

            // Respond with OP_REP_IMPORT (status=0).
            let reply = UsbIpHeader::with_status(OP_REP_IMPORT, STATUS_SUCCESS);
            sock.write_all(reply.as_bytes()).await.unwrap();

            // Send a device entry (312 bytes).
            let entry = make_device_entry_bytes("1-1");
            sock.write_all(&entry).await.unwrap();

            // Send a minimal descriptor tree (just an 18-byte device
            // descriptor is enough for the loop to read past the import
            // and start the URB forwarding loop).
            let mut desc = vec![0u8; 18];
            desc[0] = 18; // bLength
            desc[1] = 1; // DEVICE descriptor type
                         // bcdUSB, bDeviceClass, ... all zero is fine for the loop.
            sock.write_all(&desc).await.unwrap();

            // Build a USBIP_RET_SUBMIT for an IN transfer with 4 bytes
            // of data, seqnum=0xC0FFEE, devid=0, status=0.
            let ret = UsbIpRetSubmit {
                seqnum: zerocopy::byteorder::U32::new(0x00C0_FFEE),
                devid: zerocopy::byteorder::U32::new(0),
                direction: zerocopy::byteorder::U32::new(URB_DIR_IN),
                ep: zerocopy::byteorder::U32::new(0),
                status: zerocopy::byteorder::U32::new(0),
                actual_length: zerocopy::byteorder::U32::new(4),
                start_frame: zerocopy::byteorder::U32::new(0),
                number_of_packets: zerocopy::byteorder::U32::new(0),
                error_count: zerocopy::byteorder::U32::new(0),
                setup: [0u8; 8],
            };

            // Hand-encode: the client reads an 8-byte UsbIpHeader first,
            // then UsbIpRetSubmit::HEADER_SIZE (44) bytes, then data.
            let mut msg = Vec::with_capacity(8 + UsbIpRetSubmit::HEADER_SIZE + 4);
            let hdr = UsbIpHeader::new(USBIP_RET_SUBMIT);
            msg.extend_from_slice(hdr.as_bytes());
            msg.extend_from_slice(ret.as_bytes());
            msg.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
            sock.write_all(&msg).await.unwrap();
        });

        // ── The constructor under test ──────────────────────────────
        // This will fail to compile until GREEN adds Client::new_with_vhci.
        let config = ClientConfig { auto_reconnect: false, ..ClientConfig::default() };
        let client = Client::new_with_vhci(config, backend).expect("new_with_vhci must succeed");

        // Drive one URB forwarding session. The server task will close
        // the socket after sending the RET_SUBMIT, which causes the
        // client's `read_exact` to error and the loop to return Ok(()).
        let result = client.run_urb_forwarding(addr, "1-1").await;
        assert!(result.is_ok(), "run_urb_forwarding returned: {:?}", result);

        server.await.unwrap();

        // ── Observable: the injected mock recorded the completion ──
        let urbs = mock.urbs.lock().unwrap();
        assert_eq!(urbs.len(), 1, "expected exactly one completed URB on the injected mock");
        let (seqnum, devid, status, actual_len, data) = &urbs[0];
        assert_eq!(*seqnum, 0x00C0_FFEE);
        assert_eq!(*devid, 0);
        assert_eq!(*status, 0);
        assert_eq!(*actual_len, 4);
        assert_eq!(data, &vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }
}
