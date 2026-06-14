//! USB/IP Server — Export USB devices over TCP.
//!
//! ## Architecture
//!
//! ```text
//! main()
//!   ├─ mDNS thread: publishes _usbip._tcp.local
//!   ├─ TCP accept loop (port 3240)
//!   │    └─ per-client task
//!   │         ├─ handle_devlist()   → OP_REQ_DEVLIST / OP_REP_DEVLIST
//!   │         ├─ handle_import()    → OP_REQ_IMPORT / OP_REP_IMPORT
//!   │         └─ handle_urb_loop()  → USBIP_CMD_SUBMIT / USBIP_RET_SUBMIT
//!   └─ hotplug monitor: libusb hotplug callbacks
//! ```

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tracing::{debug, error, info, info_span, warn};
use uuid::Uuid;
use zerocopy::FromBytes;
use zerocopy::IntoBytes;

use usbip_core::error::ErrorKind;
use usbip_core::error::UsbIpResult;
use usbip_core::protocol::{
    UsbIpDeviceEntry, UsbIpHeader, OP_REP_DEVLIST, OP_REP_IMPORT, OP_REQ_DEVLIST, OP_REQ_IMPORT,
    STATUS_ST_DEV_BUSY, USBIP_CMD_SUBMIT,
};
use usbip_core::urb::UsbIpCmdSubmit;

use crate::api;
use crate::bandwidth::BandwidthLimit;
use crate::batcher::UrbBatcher;
use crate::discovery::MdnsAdvertiser;
use crate::urb_executor::UrbExecutor;
use crate::usb::UsbDeviceManager;

/// Global server state.
pub struct Server {
    /// USB device manager (libusb context).
    pub usb: Arc<UsbDeviceManager>,
    /// Active exports: busid -> (client_addr, device_info).
    pub exports: Arc<Mutex<HashMap<String, (SocketAddr, UsbIpDeviceEntry)>>>,
    /// mDNS advertiser.
    pub mdns: Option<MdnsAdvertiser>,
    /// Server configuration.
    pub config: ServerConfig,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_address: String,
    pub port: u16,
    pub allowed_vid_pid: Vec<(u16, u16)>,
    pub require_confirmation: bool,
    pub encryption_enabled: bool,
    pub tcp_nodelay: bool,
    /// Global default bandwidth limit (0 = unlimited).
    pub max_bandwidth: BandwidthLimit,
    /// Per-client bandwidth override (0 = use global default).
    pub per_client_bandwidth: Option<BandwidthLimit>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".into(),
            port: 3240,
            allowed_vid_pid: Vec::new(),
            require_confirmation: true,
            encryption_enabled: false,
            tcp_nodelay: true,
            max_bandwidth: BandwidthLimit::unlimited(),
            per_client_bandwidth: None,
        }
    }
}

impl Server {
    pub async fn new(config: ServerConfig) -> UsbIpResult<Self> {
        let usb = UsbDeviceManager::new()?;
        let mdns = MdnsAdvertiser::new(config.port).ok();
        Ok(Self { usb: Arc::new(usb), exports: Arc::new(Mutex::new(HashMap::new())), mdns, config })
    }

    /// Run the server — listens forever.
    pub async fn run(&self) -> UsbIpResult<()> {
        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        let listener = TcpListener::bind(&addr).await?;
        info!("USB/IP server listening on {}", addr);

        // Start mDNS advertising
        if let Some(ref mdns) = self.mdns {
            mdns.start()?;
            info!("mDNS advertising _usbip._tcp.local");
        }

        // Accept loop
        loop {
            let (stream, peer_addr) = listener.accept().await?;
            info!("Client connected from {}", peer_addr);

            let usb = self.usb.clone();
            let exports = self.exports.clone();
            let config = self.config.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_client(stream, peer_addr, usb, exports, config).await {
                    error!("Client {} error: {}", peer_addr, e);
                }
            });
        }
    }

    /// Get list of exportable devices.
    pub async fn exportable_devices(&self) -> Vec<UsbIpDeviceEntry> {
        self.usb.list_devices()
    }

    /// Run the USB/IP server together with the REST API.
    ///
    /// The API is served on `api_port` (default: 3241).
    pub async fn run_with_api(&self, api_port: u16) -> UsbIpResult<()> {
        // Build the API router
        let api_state = api::AppState {
            start_time: std::time::Instant::now(),
            exports: self.exports.clone(),
            mock_devices: None,
            config: api::ApiConfig {
                bind_address: self.config.bind_address.clone(),
                port: self.config.port,
                encryption_enabled: self.config.encryption_enabled,
            },
        };

        let api_router = api::build_router(api_state);
        let api_addr = format!("{}:{}", self.config.bind_address, api_port);

        // Start both servers in separate tasks using tokio::select!
        let usb_addr = format!("{}:{}", self.config.bind_address, self.config.port);
        let listener = TcpListener::bind(&usb_addr).await?;
        info!("USB/IP server listening on {}", usb_addr);

        // Start mDNS advertising
        if let Some(ref mdns) = self.mdns {
            mdns.start()?;
            info!("mDNS advertising _usbip._tcp.local");
        }

        // Spawn the API server
        let api_listener = tokio::net::TcpListener::bind(&api_addr).await?;
        info!("REST API listening on {}", api_addr);
        tokio::spawn(async move {
            axum::serve(api_listener, api_router).await.unwrap_or_else(|e| {
                tracing::error!("API server error: {}", e);
            });
        });

        // USB/IP accept loop (same as run())
        loop {
            let (stream, peer_addr) = listener.accept().await?;
            info!("Client connected from {}", peer_addr);

            let usb = self.usb.clone();
            let exports = self.exports.clone();
            let config = self.config.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_client(stream, peer_addr, usb, exports, config).await {
                    error!("Client {} error: {}", peer_addr, e);
                }
            });
        }
    }
}

/// Handle one TCP client connection.
async fn handle_client(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    usb: Arc<UsbDeviceManager>,
    exports: Arc<Mutex<HashMap<String, (SocketAddr, UsbIpDeviceEntry)>>>,
    config: ServerConfig,
) -> UsbIpResult<()> {
    let correlation_id = Uuid::now_v7();
    let span = info_span!("handle_client", correlation_id = %correlation_id);
    let _guard = span.enter();

    if config.tcp_nodelay {
        stream.set_nodelay(true)?;
    }

    // Read header (8 bytes)
    let mut header_buf = [0u8; 8];
    stream.read_exact(&mut header_buf).await?;

    let header = UsbIpHeader::read_from_prefix(&header_buf)
        .map_err(|_| ErrorKind::Protocol("invalid header".into()))?
        .0;

    debug!("Received command: 0x{:04x}", header.command.get());

    match header.command.get() {
        OP_REQ_DEVLIST => handle_devlist(&mut stream, &usb).await?,
        OP_REQ_IMPORT => handle_import(&mut stream, usb.clone(), &exports, peer_addr).await?,
        _ => {
            warn!("Unknown command: 0x{:04x}", header.command.get());
        },
    }

    Ok(())
}

/// Handle OP_REQ_DEVLIST: return all exportable devices.
async fn handle_devlist(stream: &mut TcpStream, usb: &UsbDeviceManager) -> UsbIpResult<()> {
    let devices = usb.list_devices();
    let ndev = devices.len() as u32;

    debug!("Sending device list: {} devices", ndev);

    // Build reply: header + ndev + device entries
    let mut reply = Vec::with_capacity(8 + 4 + ndev as usize * UsbIpDeviceEntry::SIZE);

    // Header
    let header = UsbIpHeader::new(OP_REP_DEVLIST);
    reply.extend_from_slice(header.as_bytes());

    // ndev (4 bytes, big-endian)
    reply.extend_from_slice(&ndev.to_be_bytes());

    // Device entries
    for dev in &devices {
        reply.extend_from_slice(dev.as_bytes());
    }

    stream.write_all(&reply).await?;
    stream.flush().await?;

    Ok(())
}

/// Handle OP_REQ_IMPORT: client wants to import a specific device.
async fn handle_import(
    stream: &mut TcpStream,
    usb: Arc<UsbDeviceManager>,
    exports: &Mutex<HashMap<String, (SocketAddr, UsbIpDeviceEntry)>>,
    peer_addr: SocketAddr,
) -> UsbIpResult<()> {
    // Read busid (32 bytes)
    let mut busid_buf = [0u8; 32];
    stream.read_exact(&mut busid_buf).await?;

    let busid =
        String::from_utf8_lossy(&busid_buf[..busid_buf.iter().position(|&b| b == 0).unwrap_or(32)])
            .to_string();

    info!("Client {} wants to import device: {}", peer_addr, busid);

    // Check if device exists
    let device_entry =
        usb.get_device_entry(&busid).ok_or_else(|| ErrorKind::DeviceNotFound(busid.clone()))?;

    // Check if already exported
    {
        let mut exports = exports.lock().await;
        if exports.contains_key(&busid) {
            // Send busy error
            let header = UsbIpHeader::with_status(OP_REP_IMPORT, STATUS_ST_DEV_BUSY);
            stream.write_all(header.as_bytes()).await?;
            return Ok(());
        }
        exports.insert(busid.clone(), (peer_addr, device_entry.clone()));
    }

    // Claim the device for USB/IP
    usb.claim_device(&busid)?;

    // Send OP_REP_IMPORT success with device entry + descriptor tree
    let descriptors = usb.get_descriptor_tree(&busid)?;

    let mut reply = Vec::new();
    let header = UsbIpHeader::new(OP_REP_IMPORT);
    reply.extend_from_slice(header.as_bytes());
    reply.extend_from_slice(device_entry.as_bytes());
    reply.extend_from_slice(&descriptors);

    stream.write_all(&reply).await?;

    // Enter URB forwarding loop
    handle_urb_loop(stream, usb.clone(), exports, busid, peer_addr).await
}

/// Main URB forwarding loop after device import.
async fn handle_urb_loop(
    stream: &mut TcpStream,
    usb: Arc<UsbDeviceManager>,
    exports: &Mutex<HashMap<String, (SocketAddr, UsbIpDeviceEntry)>>,
    busid: String,
    peer_addr: SocketAddr,
) -> UsbIpResult<()> {
    let correlation_id = Uuid::now_v7();
    let span =
        info_span!("urb_loop", correlation_id = %correlation_id, busid = %busid, peer = %peer_addr);
    let _guard = span.enter();

    let executor = UrbExecutor::new(usb.clone(), busid.clone());
    let mut batcher = UrbBatcher::new();
    let mut header_buf = [0u8; 8];

    loop {
        if stream.read_exact(&mut header_buf).await.is_err() {
            break;
        }

        let header = match UsbIpHeader::read_from_prefix(&header_buf) {
            Ok((h, _)) => h,
            Err(_) => break,
        };

        match header.command.get() {
            USBIP_CMD_SUBMIT => {
                let mut cmd_buf = vec![0u8; UsbIpCmdSubmit::HEADER_SIZE];
                stream.read_exact(&mut cmd_buf).await?;

                let cmd = UsbIpCmdSubmit::read_from_prefix(&cmd_buf)
                    .map_err(|_| ErrorKind::Protocol("invalid CMD_SUBMIT".into()))?
                    .0;

                let data_len = cmd.data_len() as usize;
                let mut data = vec![0u8; data_len];
                if !cmd.is_in() && data_len > 0 {
                    stream.read_exact(&mut data).await?;
                }

                let result = executor.execute(&cmd, &data);

                // Batch the reply — flush when full, non-sequential, or timed out.
                if batcher.push(&cmd, &result) {
                    let batch = batcher.flush();
                    stream.write_all(&batch).await?;
                }
            },
            _ => {
                debug!("Unknown command in URB loop: 0x{:04x}", header.command.get());
            },
        }
    }

    // Flush any remaining batched replies.
    if !batcher.is_empty() {
        let batch = batcher.flush();
        let _ = stream.write_all(&batch).await;
    }

    usb.release_device(&busid)?;
    exports.lock().await.remove(&busid);
    info!("Client {} disconnected, released {}", peer_addr, busid);
    Ok(())
}
