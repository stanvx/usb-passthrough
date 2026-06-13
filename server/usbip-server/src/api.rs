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
    routing::{get, post},
    Router,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tracing::info;
use utoipa::ToSchema;

use usbip_core::error::{CorrelationId, UsbIpError};
use usbip_core::protocol::UsbIpDeviceEntry;

// ── App state ──────────────────────────────────────────────────────

/// Shared state for the API handlers.
pub struct AppState {
    /// Server start timestamp (for uptime calculation).
    pub start_time: Instant,
    /// Active exports: busid -> (client_addr, device_info).
    pub exports: Arc<Mutex<HashMap<String, (SocketAddr, UsbIpDeviceEntry)>>>,
    /// Mock devices for testing (None = use real UsbDeviceManager).
    pub mock_devices: Option<Arc<dyn DeviceLister + Send + Sync>>,
    /// Server configuration (redacted for API responses).
    pub config: ApiConfig,
}

/// Trait abstracting device listing — injected for testability.
pub trait DeviceLister {
    fn list_devices(&self) -> Vec<UsbIpDeviceEntry>;
}

/// Server configuration exposed via the API (secrets redacted).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiConfig {
    pub bind_address: String,
    pub port: u16,
    pub encryption_enabled: bool,
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

/// Build the API router with all endpoints.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/status", get(get_status))
        .route("/api/devices", get(get_devices))
        .route("/api/config", get(get_config))
        .route("/api/connect", post(post_connect))
        .route("/api/disconnect", post(post_disconnect))
        .route("/api/events", get(ws_events))
        .route("/api/openapi.json", get(get_openapi))
        .layer(CorsLayer::permissive())
        .with_state(Arc::new(state))
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
    let devices = if let Some(ref mock) = state.mock_devices {
        mock.list_devices()
    } else {
        // TODO: use real UsbDeviceManager
        Vec::new()
    };

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
    Json(state.config.clone())
}

/// `POST /api/connect` — connect to a device on a remote server.
async fn post_connect(
    State(_state): State<Arc<AppState>>,
    Json(_req): Json<ConnectRequest>,
) -> impl IntoResponse {
    // TODO: implement remote connection logic
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiErrorResponse {
            correlation_id: CorrelationId::now_v7().to_string(),
            category: "permanent".to_string(),
            message: "Remote connection not yet implemented".to_string(),
        }),
    )
}

/// `POST /api/disconnect` — disconnect a specific device.
async fn post_disconnect(
    State(_state): State<Arc<AppState>>,
    Json(_req): Json<DisconnectRequest>,
) -> impl IntoResponse {
    // TODO: implement disconnect logic
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiErrorResponse {
            correlation_id: CorrelationId::now_v7().to_string(),
            category: "permanent".to_string(),
            message: "Disconnect not yet implemented".to_string(),
        }),
    )
}

/// `WS /api/events` — WebSocket event stream.
async fn ws_events(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws)
}

async fn handle_ws(mut socket: WebSocket) {
    info!("WebSocket client connected");

    // Send an initial heartbeat event
    let _ =
        socket.send(Message::Text(r#"{"event":"connected","data":{}}"#.to_string().into())).await;

    // Keep the connection alive by echoing pings until the client disconnects
    while let Some(msg) = socket.next().await {
        match msg {
            Ok(Message::Ping(payload)) => {
                let _ = socket.send(Message::Pong(payload)).await;
            },
            Ok(Message::Close(_)) => break,
            Err(e) => {
                info!("WebSocket error: {}", e);
                break;
            },
            _ => {},
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
