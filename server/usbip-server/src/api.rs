//! REST API + WebSocket events for the USB/IP server.
//!
//! Serves administrative endpoints on a configurable port (default: 3241)
//! alongside or separately from the USB/IP TCP port.
//!
//! ## Endpoints
//!
//! - `GET  /api/status`     — server uptime, connections, URB throughput, error count
//! - `GET  /api/devices`    — exported devices with VID, PID, status, connected client
//! - `GET  /api/config`     — server configuration (secrets redacted)
//! - `POST /api/connect`    — connect to a remote device
//! - `POST /api/disconnect` — disconnect a specific device
//! - `WS   /api/events`     — real-time event stream
//! - `GET  /api/openapi.json` — OpenAPI spec
//!
//! All errors follow the structured `UsbIpError` format with correlation IDs.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, put},
    Router,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};
use tower_http::cors::CorsLayer;
use tracing::info;
use utoipa::ToSchema;

use usbip_core::error::{ErrorKind, UsbIpError, UsbIpResult};
use usbip_core::protocol::UsbIpDeviceEntry;

/// Capacity of the latency broadcast channel. Sized so a slow WS client
/// can miss ~1s of samples before lagging.
pub const LATENCY_CHANNEL_CAPACITY: usize = 1024;

/// One URB round-trip time measurement.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LatencySample {
    pub latency_us: u64,
    pub device: String,
    pub seqnum: u64,
}

/// Build a new latency broadcast sender pair. Both ends default to the
/// same capacity as [`LATENCY_CHANNEL_CAPACITY`].
pub fn new_latency_sender() -> broadcast::Sender<LatencySample> {
    broadcast::channel(LATENCY_CHANNEL_CAPACITY).0
}

// ── App state ──────────────────────────────────────────────────────

/// Shared state for the API handlers.
pub struct AppState {
    /// Server start timestamp (for uptime calculation).
    pub start_time: Instant,
    /// Active exports: busid -> (client_addr, device_info).
    pub exports: Arc<Mutex<HashMap<String, (SocketAddr, UsbIpDeviceEntry)>>>,
    /// Source of truth for device listing — wired from `Server::usb` or
    /// from `Server::with_backend(FakeBackend)` in tests.
    pub device_lister: Arc<dyn DeviceLister + Send + Sync>,
    /// Source of truth for mDNS discovery — wired from the real
    /// `MdnsBrowser` in production, from a fake in tests.
    pub mdns_browser: Arc<dyn MdnsBrowser + Send + Sync>,
    /// Imports a device from a remote USB/IP server. Wired from a real
    /// `usbip_client::Client` in production, from a fake in tests.
    pub remote_importer: Arc<dyn RemoteImporter + Send + Sync>,
    /// Server configuration (redacted for API responses). Held in a
    /// `RwLock` so `PUT /api/config` can update without re-allocating
    /// the whole `AppState`.
    pub config: Arc<tokio::sync::RwLock<ApiConfig>>,
    /// Broadcast channel for per-URB latency samples. The URB forwarding
    /// loop in `handle_urb_loop` is the producer; the WS handler in
    /// `/api/events` is the consumer.
    pub latency_tx: broadcast::Sender<LatencySample>,
}

/// Trait abstracting device listing — injected for testability.
pub trait DeviceLister {
    fn list_devices(&self) -> Vec<UsbIpDeviceEntry>;
}

/// A USB/IP server discovered on the LAN via mDNS.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscoveredServer {
    pub host: String,
    pub port: u16,
    pub txt: HashMap<String, String>,
}

/// Trait abstracting mDNS service discovery for `_usbip._tcp.local`.
pub trait MdnsBrowser {
    /// Browse for `_usbip._tcp.local` services, waiting up to `timeout_secs`.
    fn browse(&self, timeout_secs: u32) -> Vec<DiscoveredServer>;
}

/// Trait abstracting remote device import — injected so the API handler
/// can run in tests without a real `usbip_client` (which needs `/dev/vhci`).
pub trait RemoteImporter {
    /// Import the device `busid` from `host:port`. Returns the device
    /// entry on success.
    fn import(&self, host: &str, port: u16, busid: &str) -> UsbIpResult<UsbIpDeviceEntry>;
}

/// Server configuration exposed via the API (secrets redacted).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiConfig {
    pub bind_address: String,
    pub port: u16,
    pub api_port: u16,
    pub encryption_enabled: bool,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".into(),
            port: 3240,
            api_port: 3241,
            encryption_enabled: false,
        }
    }
}

// ── Response types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct StatusResponse {
    pub status: String,
    pub uptime_seconds: f64,
    pub active_connections: usize,
    pub urb_throughput: u64,
    pub error_count: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct DeviceResponse {
    pub busid: String,
    pub vid: u16,
    pub pid: u16,
    pub speed: u32,
    pub status: String,
    pub connected_client: Option<String>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct ConnectRequest {
    pub host: String,
    pub port: u16,
    pub busid: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct ConnectResponse {
    pub busid: String,
    pub vid: u16,
    pub pid: u16,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct DisconnectRequest {
    pub busid: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct ApiErrorResponse {
    pub correlation_id: String,
    pub category: String,
    pub message: String,
}

// ── Router ─────────────────────────────────────────────────────────

/// Build the API router with all endpoints. Takes `Arc<AppState>` so
/// callers can construct the state once and reuse the router across
/// many requests (axum's `Router` is cheap to clone when sharing state).
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/status", get(get_status))
        .route("/api/devices", get(get_devices))
        .route("/api/config", get(get_config))
        .route("/api/config", put(put_config))
        .route("/api/scan", post(post_scan))
        .route("/api/connect", post(post_connect))
        .route("/api/disconnect", post(post_disconnect))
        .route("/api/events", get(ws_events))
        .route("/api/openapi.json", get(get_openapi))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ── Handlers ───────────────────────────────────────────────────────

/// `GET /api/status` — server health and metrics.
#[utoipa::path(
    get,
    path = "/api/status",
    responses(
        (status = 200, description = "Server status", body = StatusResponse),
        (status = 500, description = "Internal error", body = ApiErrorResponse),
    ),
)]
async fn get_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs_f64();
    let active = state.exports.lock().await.len();

    Json(StatusResponse {
        status: "running".to_string(),
        uptime_seconds: uptime,
        active_connections: active,
        urb_throughput: 0,
        error_count: 0,
    })
}

/// `GET /api/devices` — list exportable devices.
#[utoipa::path(
    get,
    path = "/api/devices",
    responses(
        (status = 200, description = "Device list", body = [DeviceResponse]),
    ),
)]
async fn get_devices(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let devices = state.device_lister.list_devices();

    let exports = state.exports.lock().await;

    let response: Vec<DeviceResponse> = devices
        .into_iter()
        .map(|dev| {
            let busid = dev.busid_str().to_string();
            let is_exported = exports.contains_key(&busid);
            let connected_client = exports.get(&busid).map(|(addr, _)| addr.to_string());

            DeviceResponse {
                status: if is_exported { "exported".to_string() } else { "available".to_string() },
                connected_client,
                busid,
                vid: dev.vid(),
                pid: dev.pid(),
                speed: dev.speed_val(),
                path: dev.path_str().to_string(),
            }
        })
        .collect();

    Json(response)
}

/// `GET /api/config` — current server configuration (secrets redacted).
async fn get_config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.config.read().await.clone())
}

/// Validate a port from a `serde_json::Value` (must be a u16).
fn validate_port(value: &serde_json::Value, field: &str) -> Result<u16, UsbIpError> {
    let port =
        value.as_u64().ok_or_else(|| ErrorKind::Protocol(format!("{field} must be an integer")))?;
    if port == 0 || port > u16::MAX as u64 {
        return Err(ErrorKind::Protocol(format!("{field} must be in 1..={}", u16::MAX)).into());
    }
    Ok(port as u16)
}

/// `PUT /api/config` — update and persist the server configuration.
async fn put_config(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConfigUpdateRequest>,
) -> impl IntoResponse {
    // Read the current config; we'll mutate a clone.
    let mut new_config = state.config.read().await.clone();
    if let Some(ref v) = req.port {
        match validate_port(v, "port") {
            Ok(p) => new_config.port = p,
            Err(e) => return error_response(StatusCode::BAD_REQUEST, &e),
        }
    }
    if let Some(ref v) = req.api_port {
        match validate_port(v, "api_port") {
            Ok(p) => new_config.api_port = p,
            Err(e) => return error_response(StatusCode::BAD_REQUEST, &e),
        }
    }
    if let Some(b) = req.bind_address {
        new_config.bind_address = b;
    }
    if let Some(e) = req.encryption_enabled {
        new_config.encryption_enabled = e;
    }
    if let Err(e) = persist_config(&new_config) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e);
    }
    *state.config.write().await = new_config.clone();
    Json(new_config).into_response()
}

/// Persist `ApiConfig` to disk at the platform-specific config path.
///
/// On Unix this is `$XDG_CONFIG_HOME/anyplug/server.toml` (falling back
/// to `$HOME/.config/anyplug/server.toml`); on Windows it's
/// `%APPDATA%\anyplug\server.toml`.
fn persist_config(config: &ApiConfig) -> UsbIpResult<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml = toml::to_string(config)
        .map_err(|e| ErrorKind::Protocol(format!("serialise config: {e}")))?;
    std::fs::write(&path, toml)?;
    Ok(())
}

/// Resolve the config file path. Honours `XDG_CONFIG_HOME` on Unix;
/// falls back to `%APPDATA%` on Windows and `~/Library/Application
/// Support` on macOS. The `ANYPLUG_CONFIG_PATH` env var, if set,
/// overrides everything (used by tests).
pub fn config_path() -> std::path::PathBuf {
    if let Some(p) = std::env::var_os("ANYPLUG_CONFIG_PATH") {
        return std::path::PathBuf::from(p);
    }
    let mut base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            #[cfg(target_os = "windows")]
            {
                std::env::var_os("APPDATA").map(std::path::PathBuf::from)
            }
            #[cfg(not(target_os = "windows"))]
            {
                std::env::var_os("HOME").map(|h| {
                    let mut p = std::path::PathBuf::from(h);
                    p.push(".config");
                    p
                })
            }
        })
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.push("anyplug");
    base.push("server.toml");
    base
}

/// Load `ApiConfig` from disk if present. Missing file → defaults.
pub fn load_config() -> ApiConfig {
    let path = config_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return ApiConfig::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

/// Request body for `POST /api/scan`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub(crate) struct ScanRequest {
    /// Browse timeout in seconds. Default 5, clamped to [1, 30].
    #[serde(default)]
    pub timeout_secs: Option<u32>,
}

/// Request body for `PUT /api/config`.
///
/// Uses `serde_json::Value` for `port` / `api_port` so an out-of-range
/// value (e.g. 0 or 70000) is caught here and turned into a structured
/// 400 response, instead of failing JSON deserialization with a plain
/// 422.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub(crate) struct ConfigUpdateRequest {
    pub bind_address: Option<String>,
    pub port: Option<serde_json::Value>,
    pub api_port: Option<serde_json::Value>,
    pub encryption_enabled: Option<bool>,
}

/// `POST /api/scan` — discover USB/IP servers on the LAN via mDNS.
#[utoipa::path(
    post,
    path = "/api/scan",
    request_body = ScanRequest,
    responses(
        (status = 200, description = "Discovered servers", body = [DiscoveredServer]),
        (status = 500, description = "Scan failed", body = ApiErrorResponse),
    ),
)]
async fn post_scan(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ScanRequest>,
) -> impl IntoResponse {
    let timeout = req.timeout_secs.unwrap_or(5).clamp(1, 30);
    let servers = state.mdns_browser.browse(timeout);
    Json(servers)
}

/// `POST /api/connect` — connect to a device on a remote server.
#[utoipa::path(
    post,
    path = "/api/connect",
    request_body = ConnectRequest,
    responses(
        (status = 200, description = "Imported device", body = ConnectResponse),
        (status = 500, description = "Import failed", body = ApiErrorResponse),
    ),
)]
async fn post_connect(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConnectRequest>,
) -> impl IntoResponse {
    match state.remote_importer.import(&req.host, req.port, &req.busid) {
        Ok(entry) => {
            let busid = entry.busid_str().to_string();
            let vid = entry.vid();
            let pid = entry.pid();
            // Record the export under the loopback peer address — the
            // remote side is identified by host:port in the request.
            let peer = format!("{}:{}", req.host, req.port)
                .parse()
                .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 0)));
            state.exports.lock().await.insert(busid.clone(), (peer, entry));
            (StatusCode::OK, Json(ConnectResponse { busid, vid, pid, status: "imported" }))
                .into_response()
        },
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// Build a structured error response with correlation ID.
fn error_response(status: StatusCode, err: &UsbIpError) -> axum::response::Response {
    (
        status,
        Json(ApiErrorResponse {
            correlation_id: err.correlation_id().to_string(),
            category: err.category().to_string(),
            message: format!("{}", err.kind()),
        }),
    )
        .into_response()
}

/// `POST /api/disconnect` — disconnect a specific device.
async fn post_disconnect(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DisconnectRequest>,
) -> impl IntoResponse {
    let mut exports = state.exports.lock().await;
    match exports.remove(&req.busid) {
        Some(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"busid": req.busid, "status": "disconnected"})),
        )
            .into_response(),
        None => {
            let err = UsbIpError::from(ErrorKind::DeviceNotFound(req.busid.clone()));
            error_response(StatusCode::NOT_FOUND, &err)
        },
    }
}

/// `WS /api/events` — WebSocket event stream.
async fn ws_events(State(state): State<Arc<AppState>>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>) {
    info!("WebSocket client connected");

    // Send an initial heartbeat event.
    let _ =
        socket.send(Message::Text(r#"{"event":"connected","data":{}}"#.to_string().into())).await;

    // Subscribe to the latency broadcast channel.
    let mut latency_rx = state.latency_tx.subscribe();

    loop {
        tokio::select! {
            // Forward incoming WS messages (pings, closes) so we react
            // to client disconnects.
            ws_msg = socket.next() => {
                match ws_msg {
                    Some(Ok(Message::Ping(payload))) => {
                        let _ = socket.send(Message::Pong(payload)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        info!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            // Forward broadcast latency samples as text frames.
            sample = latency_rx.recv() => {
                match sample {
                    Ok(s) => {
                        let frame = serde_json::json!({
                            "type": "latency",
                            "payload": s,
                        })
                        .to_string();
                        if socket.send(Message::Text(frame.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // A slow consumer fell behind; skip missed samples
                        // and continue from the next one.
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    info!("WebSocket client disconnected");
}

/// `GET /api/openapi.json` — OpenAPI spec.
async fn get_openapi() -> impl IntoResponse {
    let spec = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/api/openapi.json"));
    (StatusCode::OK, [("content-type", "application/json")], spec)
}

/// Convert a `UsbIpError` into an API error response.
#[allow(dead_code)]
fn to_api_error(err: &UsbIpError) -> ApiErrorResponse {
    ApiErrorResponse {
        correlation_id: err.correlation_id().to_string(),
        category: err.category().to_string(),
        message: format!("{}", err.kind()),
    }
}
