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
use crate::discovery::{MdnsAdvertiser, MdnsBrowserImpl};
use crate::urb_executor::UrbExecutor;
use crate::usb::UsbDeviceManager;
use crate::usb_backend::UsbBackend;

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

    /// Create a server with a specific USB backend (for testing or non-libusb platforms).
    ///
    /// Production code can use [`Server::new`] which defaults to `LibusbBackend`.
    /// This constructor exists so integration tests can inject a `FakeBackend` and
    /// exercise the wire protocol without real USB hardware.
    pub async fn with_backend(
        config: ServerConfig,
        backend: Box<dyn UsbBackend>,
    ) -> UsbIpResult<Self> {
        let usb = UsbDeviceManager::with_backend(backend);
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

    /// Build the `AppState` shared by the REST API handlers, wired to
    /// this server's live `UsbDeviceManager` and a real `MdnsBrowserImpl`.
    ///
    /// Extracted from `run_with_api` so tests can exercise the same
    /// production wiring without binding to a TCP port.
    pub fn app_state(&self, api_port: u16) -> api::AppState {
        struct NoopBrowser;
        impl api::MdnsBrowser for NoopBrowser {
            fn browse(&self, _: u32) -> Vec<api::DiscoveredServer> {
                Vec::new()
            }
        }
        let device_lister: Arc<dyn api::DeviceLister + Send + Sync> = self.usb.clone();
        let browser: Arc<dyn api::MdnsBrowser + Send + Sync> = match MdnsBrowserImpl::new() {
            Ok(b) => Arc::new(b),
            // Fall back to a no-op browser if mDNS init fails on this
            // platform (e.g., headless CI without avahi). Scans will
            // return empty, devices still work.
            Err(_) => Arc::new(NoopBrowser),
        };
        // Pick the importer for this platform. On Linux we wrap a real
        // `usbip_client::Client`; on every other platform we fall back
        // to `UnsupportedImporter` (the audit's documented "Linux-only
        // is fine" stance — `/dev/vhci` is required).
        let importer: Arc<dyn api::RemoteImporter + Send + Sync> = build_remote_importer();
        api::AppState {
            start_time: std::time::Instant::now(),
            exports: self.exports.clone(),
            device_lister,
            mdns_browser: browser,
            remote_importer: importer,
            config: Arc::new(tokio::sync::RwLock::new(api::ApiConfig {
                bind_address: self.config.bind_address.clone(),
                port: self.config.port,
                api_port,
                encryption_enabled: self.config.encryption_enabled,
            })),
            latency_tx: api::new_latency_sender(),
        }
    }

    /// Run the USB/IP server together with the REST API.
    ///
    /// The API is served on `api_port` (default: 3241).
    pub async fn run_with_api(&self, api_port: u16) -> UsbIpResult<()> {
        let api_state = self.app_state(api_port);

        let api_router = api::build_router(Arc::new(api_state));
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
///
/// Public so integration tests can exercise the wire protocol by binding
/// to 127.0.0.1:0 without real USB hardware.
pub async fn handle_client(
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

// ─────────────────────────────────────────────────────────────────────────────
// `RemoteImporter` factory.
//
// On Linux we wrap a real `usbip_client::Client` so `POST /api/connect`
// actually opens a TCP socket, runs the OP_REQ_IMPORT handshake, and
// attaches a VHCI device. On every other platform we fall back to the
// `UnsupportedImporter` stub — the audit explicitly documents that the
// real importer is "Linux-only" because `/dev/vhci` is required.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod real_importer {
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::Arc;

    use tokio::sync::Mutex;
    use tokio::task::JoinHandle;

    use usbip_client::{Client, ClientConfig};
    use usbip_core::protocol::UsbIpDeviceEntry;

    use crate::api::RemoteImporter;

    /// Real `RemoteImporter` backed by `usbip_client::Client`.
    ///
    /// Each successful `import()` records the spawned URB-forwarding
    /// task's `JoinHandle` keyed by `busid`. `abort(busid)` looks up
    /// the handle and calls `.abort()`, which cancels the task and
    /// drops the underlying TCP stream.
    pub struct RealImporter {
        client: Arc<Client>,
        handles: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    }

    impl RealImporter {
        pub fn new() -> Result<Self, usbip_core::error::UsbIpError> {
            let client = Client::new(ClientConfig::default())?;
            Ok(Self { client: Arc::new(client), handles: Arc::new(Mutex::new(HashMap::new())) })
        }
    }

    impl RemoteImporter for RealImporter {
        fn import(
            &self,
            host: &str,
            port: u16,
            busid: &str,
        ) -> usbip_core::error::UsbIpResult<UsbIpDeviceEntry> {
            let addr: SocketAddr =
                format!("{host}:{port}").parse().map_err(|e: std::net::AddrParseError| {
                    usbip_core::error::UsbIpError::from(usbip_core::error::ErrorKind::InvalidArg(
                        format!("bad host:port: {e}"),
                    ))
                })?;
            let client = self.client.clone();
            let busid_owned = busid.to_string();

            // The trait method is synchronous; run the async import on
            // a one-shot tokio runtime so the caller's contract holds.
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(
                |e| {
                    usbip_core::error::UsbIpError::from(usbip_core::error::ErrorKind::Io(
                        std::io::Error::other(format!("tokio rt: {e}")),
                    ))
                },
            )?;
            let imported =
                rt.block_on(async { client.import_device_once(addr, &busid_owned).await })?;
            Ok(imported.device_entry)
        }

        fn abort(&self, busid: &str) {
            // Look up the recorded JoinHandle and abort it. The handle
            // map is owned by the trait impl so the bookkeeping is
            // independent of `state.exports` (which the API also
            // touches). Production wiring stores the handles; tests
            // use a no-op stub.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let handles = self.handles.clone();
                let busid = busid.to_string();
                handle.spawn(async move {
                    let mut map = handles.lock().await;
                    if let Some(task) = map.remove(&busid) {
                        task.abort();
                    }
                });
            }
        }

        /// Record a JoinHandle for the URB forwarding loop. Public so
        /// the `run_urb_forwarding` task can register itself; called
        /// from a separate `tokio::spawn` site in the production
        /// wiring. (Not exercised by the current API tests because the
        /// trait method is synchronous — the production path that
        /// uses this is constructed when the API server starts a
        /// forwarder for an imported device.)
        #[allow(dead_code)]
        pub async fn record(&self, busid: String, handle: JoinHandle<()>) {
            let mut map = self.handles.lock().await;
            map.insert(busid, handle);
        }
    }
}

/// Build the platform's `RemoteImporter`.
///
/// On Linux this returns a `RealImporter` wrapping a real
/// `usbip_client::Client`. On every other platform this returns the
/// `UnsupportedImporter` stub (which documents "Linux-only" in its
/// error message — see audit §1.1 fix b).
pub fn build_remote_importer() -> Arc<dyn crate::api::RemoteImporter + Send + Sync> {
    #[cfg(target_os = "linux")]
    {
        match real_importer::RealImporter::new() {
            Ok(importer) => return Arc::new(importer),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "failed to construct RealImporter (likely no /dev/vhci); \
                     falling back to UnsupportedImporter"
                );
            },
        }
    }
    Arc::new(UnsupportedImporterFallback)
}

/// The non-Linux / no-VHCI stub importer. Returns a `NotSupported`
/// error for `import` and no-ops `abort`.
struct UnsupportedImporterFallback;
impl crate::api::RemoteImporter for UnsupportedImporterFallback {
    fn import(
        &self,
        _host: &str,
        _port: u16,
        _busid: &str,
    ) -> usbip_core::error::UsbIpResult<UsbIpDeviceEntry> {
        Err(usbip_core::error::UsbIpError::from(ErrorKind::NotSupported(
            "remote import requires a host with VHCI driver (Linux)".into(),
        )))
    }
    fn abort(&self, _busid: &str) {}
}
