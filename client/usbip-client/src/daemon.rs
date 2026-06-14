//! USB/IP Client Daemon — persistent background service.
//!
//! Runs as a systemd service, auto-connects to configured servers/devices,
//! survives login/logout cycles, and exposes a Unix domain socket for CLI
//! control (connect/disconnect/status commands).
//!
//! ## Architecture
//!
//! ```text
//! systemd → usbip-client --daemon
//!              │
//!              ├─ Unix socket listener (/var/run/usbip-client.sock)
//!              │    handles: connect, disconnect, status, shutdown
//!              │
//!              ├─ config watcher (/etc/usbip-client/config.toml)
//!              │
//!              └─ auto-connect tasks (one per configured device)
//!                   reconnects with exponential backoff (1s..16s max)
//! ```
//!
//! The daemon does **not** run the existing foreground `connect` code path.
//! It manages its own connections with automatic reconnection logic.

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use usbip_core::error::*;

use crate::client::{Client, ClientConfig};

// ── Configuration ────────────────────────────────────────────────────────

/// Daemon configuration file format (TOML).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DaemonConfig {
    /// Socket path for the Unix domain socket.
    #[serde(default = "default_socket_path")]
    pub socket_path: PathBuf,
    /// List of servers/devices to auto-connect.
    #[serde(default)]
    pub connections: Vec<ConnectionConfig>,
}

/// A single auto-connect target.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionConfig {
    /// Server address (host:port).
    pub server: SocketAddr,
    /// USB bus ID to import.
    pub busid: String,
}

fn default_socket_path() -> PathBuf {
    let dir = default_run_dir();
    dir.join("usbip-client.sock")
}

fn default_run_dir() -> PathBuf {
    // Prefer /var/run for system-wide operation; fall back to user runtime dir.
    let system_run = PathBuf::from("/var/run/usbip-client");
    if system_run.exists() {
        return system_run;
    }
    if let Some(runtime) = dirs::runtime_dir() {
        runtime.join("usbip-client")
    } else if let Some(data) = dirs::data_local_dir() {
        data.join("usbip-client")
    } else {
        PathBuf::from("/tmp/usbip-client")
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            connections: Vec::new(),
        }
    }
}

impl DaemonConfig {
    /// Load config from a TOML file. Returns `Ok(Default)` if the file
    /// doesn't exist (so the daemon can still start without a config).
    pub fn load(path: &Path) -> UsbIpResult<Self> {
        if !path.exists() {
            info!("No config file at {}, using defaults", path.display());
            return Ok(Self::default());
        }
        let content =
            fs::read_to_string(path).map_err(ErrorKind::Io)?;
        let config: Self = toml::from_str(&content)
            .map_err(|e| ErrorKind::InvalidMessage(format!("bad config: {}", e)))?;
        Ok(config)
    }

    /// Default config paths, searched in order.
    pub fn default_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        paths.push(PathBuf::from("/etc/usbip-client/config.toml"));
        if let Some(config) = dirs::config_dir() {
            paths.push(config.join("usbip-client").join("config.toml"));
        }
        paths
    }

    /// Find the first existing config file, or return None.
    pub fn find_config() -> Option<PathBuf> {
        Self::default_paths().into_iter().find(|p| p.exists())
    }
}

// ── Unix socket protocol messages ───────────────────────────────────────

/// Command sent from CLI to daemon over the Unix socket.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "command", content = "args")]
pub enum DaemonCommand {
    /// Connect to a server and import a device.
    Connect {
        server: SocketAddr,
        busid: String,
    },
    /// Disconnect an imported device.
    Disconnect {
        busid: String,
    },
    /// Query daemon status (connected devices, etc.).
    Status,
    /// Gracefully shut down the daemon.
    Shutdown,
}

/// Response from daemon to CLI.
#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connections: Option<Vec<ConnectionStatus>>,
}

/// Status of a single connection managed by the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub server: SocketAddr,
    pub busid: String,
    pub state: String, // "connected", "connecting", "disconnected", "error"
    pub error: Option<String>,
}

// ── Daemon manager ──────────────────────────────────────────────────────

/// Tracks a single managed device connection.
#[derive(Debug)]
struct ManagedConnection {
    config: ConnectionConfig,
    state: ConnectionState,
    /// Handle to cancel the connection task.
    cancel_token: tokio_util::sync::CancellationToken,
}

#[derive(Debug, Clone, PartialEq)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// The main daemon manager.
pub struct DaemonManager {
    config: DaemonConfig,
    client: Arc<Client>,
    connections: Arc<Mutex<Vec<ManagedConnection>>>,
    shutdown_token: tokio_util::sync::CancellationToken,
}

impl DaemonManager {
    /// Create a new daemon manager with the given config.
    pub fn new(config: DaemonConfig) -> UsbIpResult<Self> {
        let client_config = ClientConfig {
            use_mdns: false,
            auto_reconnect: false, // daemon manages reconnect itself
            ..Default::default()
        };
        let client = Client::new(client_config)?;
        Ok(Self {
            config,
            client: Arc::new(client),
            connections: Arc::new(Mutex::new(Vec::new())),
            shutdown_token: tokio_util::sync::CancellationToken::new(),
        })
    }

    /// Run the daemon: start the Unix socket listener and auto-connect tasks.
    pub async fn run(&self) -> UsbIpResult<()> {
        // Ensure socket directory exists
        if let Some(parent) = self.config.socket_path.parent() {
            fs::create_dir_all(parent)
                .map_err(ErrorKind::Io)?;
        }

        // Remove stale socket file if present
        if self.config.socket_path.exists() {
            fs::remove_file(&self.config.socket_path)
                .map_err(ErrorKind::Io)?;
        }

        // Start Unix socket listener
        let listener = UnixListener::bind(&self.config.socket_path)
            .map_err(ErrorKind::Io)?;

        info!(
            "Daemon listening on {}",
            self.config.socket_path.display()
        );

        // Start auto-connect for configured devices
        for conn_cfg in &self.config.connections {
            self.start_auto_connect(conn_cfg.clone());
        }

        // Accept incoming Unix socket connections
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            let connections = self.connections.clone();
                            let shutdown_token = self.shutdown_token.clone();
                            let client = self.client.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, client, connections, shutdown_token).await {
                                    error!("Socket handler error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept Unix socket connection: {}", e);
                        }
                    }
                }
                _ = self.shutdown_token.cancelled() => {
                    info!("Daemon shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Start an auto-connect task for the given connection config.
    fn start_auto_connect(&self, conn_cfg: ConnectionConfig) {
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let client = self.client.clone();
        let connections = self.connections.clone();
        let shutdown_token = self.shutdown_token.clone();
        let cfg = conn_cfg.clone();

        // Add to managed connections
        {
            let mut conns = connections.blocking_lock();
            conns.push(ManagedConnection {
                config: conn_cfg,
                state: ConnectionState::Disconnected,
                cancel_token: cancel_token.clone(),
            });
        }

        tokio::spawn(async move {
            auto_connect_loop(
                cfg,
                client,
                connections,
                cancel_token,
                shutdown_token,
            )
            .await;
        });
    }

    /// Gracefully shut down the daemon.
    pub fn shutdown(&self) {
        info!("Initiating daemon shutdown");
        self.shutdown_token.cancel();
    }
}

// ── Auto-connect loop ───────────────────────────────────────────────────

/// Infinite auto-connect loop with exponential backoff.
async fn auto_connect_loop(
    config: ConnectionConfig,
    client: Arc<Client>,
    connections: Arc<Mutex<Vec<ManagedConnection>>>,
    cancel_token: tokio_util::sync::CancellationToken,
    shutdown_token: tokio_util::sync::CancellationToken,
) {
loop {
    // Check for shutdown or cancel
        tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("Auto-connect cancelled for {} ({})", config.server, config.busid);
                return;
            }
            _ = shutdown_token.cancelled() => {
                return;
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
        }

        // Update state to Connecting
        {
            let mut conns = connections.lock().await;
            if let Some(mc) = conns.iter_mut().find(|c| c.config.busid == config.busid && c.config.server == config.server) {
                mc.state = ConnectionState::Connecting;
            }
        }

        info!(
            "Connecting to {} (busid={})...",
            config.server, config.busid
        );

        match client.import_device(config.server, &config.busid).await {
            Ok(device) => {
                info!(
                    "Connected to {} (busid={}, device={:04x}:{:04x})",
                    config.server,
                    config.busid,
                    device.device_entry.vid(),
                    device.device_entry.pid(),
                );

                // Update state to Connected
                {
                    let mut conns = connections.lock().await;
                    if let Some(mc) = conns.iter_mut().find(|c| c.config.busid == config.busid && c.config.server == config.server) {
                        mc.state = ConnectionState::Connected;
                    }
                }

                // Wait for disconnect/cancel/shutdown signal
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        info!("Disconnecting {} ({})", config.server, config.busid);
                        return;
                    }
                    _ = shutdown_token.cancelled() => {
                        return;
                    }
                    // If we get here, the connection dropped (client.import_device spawns
                    // the URB forwarding task which returns on disconnect).
                    // We detect this by polling the cancellation token.
                    _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                        // Check if connection still alive by looking at state
                        let still_connected = {
                            let conns = connections.lock().await;
                            conns.iter()
                                .any(|c| c.config.busid == config.busid
                                     && c.config.server == config.server
                                     && c.state == ConnectionState::Connected)
                        };
                        if !still_connected {
                            // Connection was explicitly disconnected
                            return;
                        }
                        // Connection dropped — reconnect with backoff
                        warn!("Connection to {} ({}) dropped, reconnecting...", config.server, config.busid);
                    }
                }
            }
            Err(e) => {
                error!(
                    "Failed to connect to {} (busid={}): {}",
                    config.server, config.busid, e
                );

                // Update state to Error
                {
                    let mut conns = connections.lock().await;
                    if let Some(mc) = conns.iter_mut().find(|c| c.config.busid == config.busid && c.config.server == config.server) {
                        mc.state = ConnectionState::Error(format!("{}", e));
                    }
                }

                // Exponential backoff: 1s, 2s, 4s, 8s, 16s capped
                let delay = std::time::Duration::from_secs(1);
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        return;
                    }
                    _ = shutdown_token.cancelled() => {
                        return;
                    }
                    _ = tokio::time::sleep(delay) => {
                        // Backoff complete, retry
                    }
                }
                info!("Retrying connection to {} (busid={})...", config.server, config.busid);
            }
        }
    }
}

// ── Unix domain socket handler ──────────────────────────────────────────

/// Handle a single CLI connection over the Unix socket.
async fn handle_connection(
    mut stream: UnixStream,
    client: Arc<Client>,
    connections: Arc<Mutex<Vec<ManagedConnection>>>,
    shutdown_token: tokio_util::sync::CancellationToken,
) -> UsbIpResult<()> {
    let (reader, mut writer) = stream.split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    // Read one line per request (JSON per line)
    loop {
        line.clear();
        let bytes_read = buf_reader
            .read_line(&mut line)
            .await
            .map_err(ErrorKind::Io)?;

        if bytes_read == 0 {
            break; // client disconnected
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<DaemonCommand>(trimmed) {
            Ok(cmd) => handle_command(cmd, &client, &connections, &shutdown_token).await,
            Err(e) => DaemonResponse {
                status: "error".to_string(),
                message: Some(format!("invalid command: {}", e)),
                connections: None,
            },
        };

        let response_json = serde_json::to_string(&response)
            .map_err(|e| ErrorKind::Serialization(format!("serialization error: {}", e)))?;

        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        // If shutdown was requested, stop handling further commands
        if shutdown_token.is_cancelled() {
            break;
        }
    }

    Ok(())
}

/// Process a single daemon command.
async fn handle_command(
    cmd: DaemonCommand,
    client: &Arc<Client>,
    connections: &Arc<Mutex<Vec<ManagedConnection>>>,
    shutdown_token: &tokio_util::sync::CancellationToken,
) -> DaemonResponse {
    match cmd {
        DaemonCommand::Connect { server, busid } => {
            // Check if already connected
            {
                let conns = connections.lock().await;
                if conns.iter().any(|c| c.config.busid == busid && c.config.server == server) {
                    return DaemonResponse {
                        status: "ok".to_string(),
                        message: Some(format!("already managing {} from {}", busid, server)),
                        connections: None,
                    };
                }
            }

            // Connect now in foreground (blocking)
            match client.import_device(server, &busid).await {
                Ok(device) => {
                    info!(
                        "Connected to {} (busid={}, device={:04x}:{:04x})",
                        server,
                        busid,
                        device.device_entry.vid(),
                        device.device_entry.pid(),
                    );

                    // Add to managed connections
                    let cancel_token = tokio_util::sync::CancellationToken::new();
                    let client_clone = client.clone();
                    let connections_clone = connections.clone();
                    let shutdown_token_clone = shutdown_token.clone();
                    let cfg = ConnectionConfig {
                        server,
                        busid: busid.clone(),
                    };

                    {
                        let mut conns = connections.lock().await;
                        conns.push(ManagedConnection {
                            config: cfg.clone(),
                            state: ConnectionState::Connected,
                            cancel_token: cancel_token.clone(),
                        });
                    }

                    // Spawn monitor task for auto-reconnect
                    tokio::spawn(async move {
                        auto_connect_loop(
                            cfg,
                            client_clone,
                            connections_clone,
                            cancel_token,
                            shutdown_token_clone,
                        )
                        .await;
                    });

                    DaemonResponse {
                        status: "ok".to_string(),
                        message: Some(format!(
                            "imported {:04x}:{:04x} from {}",
                            device.device_entry.vid(),
                            device.device_entry.pid(),
                            server,
                        )),
                        connections: None,
                    }
                }
                Err(e) => DaemonResponse {
                    status: "error".to_string(),
                    message: Some(format!("import failed: {}", e)),
                    connections: None,
                },
            }
        }

        DaemonCommand::Disconnect { busid } => {
            let mut conns = connections.lock().await;
            if let Some(pos) = conns.iter().position(|c| c.config.busid == busid) {
                let mc = conns.remove(pos);
                mc.cancel_token.cancel();
                info!("Disconnected device {}", busid);
                DaemonResponse {
                    status: "ok".to_string(),
                    message: Some(format!("disconnected {}", busid)),
                    connections: None,
                }
            } else {
                DaemonResponse {
                    status: "error".to_string(),
                    message: Some(format!("no connection found for busid {}", busid)),
                    connections: None,
                }
            }
        }

        DaemonCommand::Status => {
            let conns = connections.lock().await;
            let status_list: Vec<ConnectionStatus> = conns
                .iter()
                .map(|c| ConnectionStatus {
                    server: c.config.server,
                    busid: c.config.busid.clone(),
                    state: match &c.state {
                        ConnectionState::Disconnected => "disconnected".to_string(),
                        ConnectionState::Connecting => "connecting".to_string(),
                        ConnectionState::Connected => "connected".to_string(),
                        ConnectionState::Error(e) => format!("error: {}", e),
                    },
                    error: match &c.state {
                        ConnectionState::Error(e) => Some(e.clone()),
                        _ => None,
                    },
                })
                .collect();

            DaemonResponse {
                status: "ok".to_string(),
                message: None,
                connections: Some(status_list),
            }
        }

        DaemonCommand::Shutdown => {
            info!("Shutdown requested via Unix socket");
            shutdown_token.cancel();
            DaemonResponse {
                status: "ok".to_string(),
                message: Some("shutting down".to_string()),
                connections: None,
            }
        }
    }
}

// ── Control client (used by the CLI to talk to the daemon) ──────────────

/// Connect to a running daemon's Unix socket and send a command.
/// Returns the parsed response.
pub async fn send_daemon_command(
    socket_path: &Path,
    cmd: &DaemonCommand,
) -> UsbIpResult<DaemonResponse> {
    let mut stream = UnixStream::connect(socket_path)
        .await
        .map_err(ErrorKind::Io)?;

    let cmd_json = serde_json::to_string(cmd)
        .map_err(|e| ErrorKind::Serialization(format!("serialization error: {}", e)))?;

    stream.write_all(cmd_json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

    // Read response (one line)
    let mut buf = String::new();
    let mut reader = BufReader::new(&mut stream);
    reader
        .read_line(&mut buf)
        .await
        .map_err(ErrorKind::Io)?;

        let response: DaemonResponse= serde_json::from_str(buf.trim())
        .map_err(|e| ErrorKind::InvalidMessage(format!("invalid response: {}", e)))?;

    Ok(response)
}
