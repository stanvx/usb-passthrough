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
use crate::vhci::VhciDriver;

/// Client configuration.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Server address (if manual connection).
    pub server_addr: Option<SocketAddr>,
    /// Use mDNS for discovery.
    pub use_mdns: bool,
    /// Reconnect on disconnect.
    pub auto_reconnect: bool,
    /// Number of reconnect attempts.
    pub reconnect_attempts: u32,
    /// Delay between reconnect attempts (ms).
    pub reconnect_delay_ms: u64,
    /// TCP nodelay.
    pub tcp_nodelay: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_addr: None,
            use_mdns: true,
            auto_reconnect: true,
            reconnect_attempts: 3,
            reconnect_delay_ms: 1000,
            tcp_nodelay: true,
        }
    }
}

/// USB/IP client.
pub struct Client {
    config: ClientConfig,
    vhci: Arc<VhciDriver>,
    /// Active connections: busid -> (stream, device_info).
    connections: Mutex<Vec<ActiveConnection>>,
}

struct ActiveConnection {
    busid: String,
    device_entry: UsbIpDeviceEntry,
    descriptors: Vec<u8>,
}

impl Client {
    pub fn new(config: ClientConfig) -> UsbIpResult<Self> {
        let vhci = VhciDriver::new()?;
        Ok(Self { config, vhci: Arc::new(vhci), connections: Mutex::new(Vec::new()) })
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

    /// Import a USB device from a server.
    ///
    /// This creates a virtual USB device on the local system via VHCI.
    /// The returned handle lets you monitor/detach the device.
    pub async fn import_device(
        &self,
        addr: SocketAddr,
        busid: &str,
    ) -> UsbIpResult<ImportedDevice> {
        let correlation_id = Uuid::now_v7();
        let span = info_span!("import_device", correlation_id = %correlation_id, busid = %busid, server = %addr);
        let _guard = span.enter();

        let mut stream = TcpStream::connect(addr).await?;
        if self.config.tcp_nodelay {
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
            Err(_) => {
                return Err(UsbIpError::from(ErrorKind::Protocol("invalid import reply".into())))
            },
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
            Err(_) => {
                return Err(UsbIpError::from(ErrorKind::Protocol("invalid device entry".into())))
            },
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
        let _vhci_device = self.vhci.create_device(&device_entry, &descriptors)?;

        // Store connection
        {
            let mut conns = self.connections.lock().await;
            conns.push(ActiveConnection {
                busid: busid.to_string(),
                device_entry: device_entry.clone(),
                descriptors: descriptors.clone(),
            });
        }

        info!("Imported device: {} ({}:{})", busid, addr, busid);

        // Spawn URB forwarding task
        let vhci = self.vhci.clone();
        let config = self.config.clone();
        let busid_owned = busid.to_string();
        let addr_owned = addr;

        tokio::spawn(async move {
            if let Err(e) = urb_forwarding_loop(stream, vhci, busid_owned, addr_owned).await {
                error!("URB forwarding error: {}", e);

                // Auto-reconnect
                if config.auto_reconnect {
                    for attempt in 1..=config.reconnect_attempts {
                        warn!(
                            "Reconnecting to {} (attempt {}/{})",
                            addr_owned, attempt, config.reconnect_attempts
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(
                            config.reconnect_delay_ms,
                        ))
                        .await;
                    }
                }
            }
        });

        Ok(ImportedDevice { busid: busid.to_string(), device_entry })
    }
}

/// Handle for an imported device.
#[derive(Debug, Clone)]
pub struct ImportedDevice {
    pub busid: String,
    pub device_entry: UsbIpDeviceEntry,
}

/// Main URB forwarding loop -- bidirectional proxy between VHCI and server.
async fn urb_forwarding_loop(
    mut stream: TcpStream,
    vhci: Arc<VhciDriver>,
    _busid: String,
    _server_addr: SocketAddr,
) -> UsbIpResult<()> {
    let correlation_id = Uuid::now_v7();
    let span = info_span!("urb_forwarding_loop", correlation_id = %correlation_id, busid = %_busid, server = %_server_addr);
    let _guard = span.enter();

    let mut header_buf = [0u8; 8];
    let _seqnum: u32 = 0;

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
