//! Integration tests for the REST API module.
//!
//! These tests spin up a minimal server with a mock device manager and
//! issue HTTP requests to verify response shapes and status codes.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::StatusCode;
use tokio::sync::Mutex;

use usbip_core::protocol::{UsbIpDeviceEntry, U16BE, U32BE};
use usbip_server::api::DeviceLister;

// ── Mock device manager ────────────────────────────────────────────

struct MockDeviceManager;

impl DeviceLister for MockDeviceManager {
    fn list_devices(&self) -> Vec<UsbIpDeviceEntry> {
        vec![make_device_entry("1-1", 0x046d, 0xc261), make_device_entry("1-2", 0x8087, 0x0024)]
    }
}

fn make_device_entry(busid: &str, vid: u16, pid: u16) -> UsbIpDeviceEntry {
    let mut entry = UsbIpDeviceEntry {
        path: [0u8; 256],
        busid: [0u8; 32],
        busnum: U32BE::new(1),
        devnum: U32BE::new(1),
        speed: U32BE::new(3),
        id_vendor: U16BE::new(vid),
        id_product: U16BE::new(pid),
        bcd_device: U16BE::new(0x0100),
        b_device_class: 0,
        b_device_sub_class: 0,
        b_device_protocol: 0,
        b_configuration_value: 1,
        b_num_configurations: 1,
        b_num_interfaces: 1,
    };
    let busid_bytes = busid.as_bytes();
    let copy_len = busid_bytes.len().min(31);
    entry.busid[..copy_len].copy_from_slice(&busid_bytes[..copy_len]);
    let path = format!("/sys/bus/usb/devices/{}", busid);
    let path_bytes = path.as_bytes();
    let copy_len = path_bytes.len().min(255);
    entry.path[..copy_len].copy_from_slice(&path_bytes[..copy_len]);
    entry
}

// ── Test fixture ───────────────────────────────────────────────────

/// Helper to build an app for testing.
/// Returns an axum `Router` with a mock device manager injected.
fn test_app() -> axum::Router {
    test_app_with_exports(HashMap::new())
}

/// Build a test app with pre-populated exports map.
fn test_app_with_exports(exports: HashMap<String, (SocketAddr, UsbIpDeviceEntry)>) -> axum::Router {
    use usbip_server::api;

    let state = api::AppState {
        start_time: std::time::Instant::now(),
        exports: Arc::new(Mutex::new(exports)),
        mock_devices: Some(Arc::new(MockDeviceManager)),
        config: api::ApiConfig {
            bind_address: "0.0.0.0".to_string(),
            port: 3240,
            encryption_enabled: false,
        },
    };
    api::build_router(state)
}

/// Convenience: issue a GET and return the JSON body as a `serde_json::Value`.
async fn get_json(router: axum::Router, path: &str) -> (StatusCode, serde_json::Value) {
    use axum::body::Body;
    use tower::util::ServiceExt;

    let req = axum::http::Request::builder().uri(path).method("GET").body(Body::empty()).unwrap();

    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes)
        .unwrap_or_else(|e| panic!("JSON parse error on {}: {} — body: {:?}", path, e, body_bytes));
    (status, json)
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Convenience: issue a POST with JSON body and return the response.
async fn post_json(
    router: axum::Router,
    path: &str,
    body: &serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    use axum::body::Body;
    use tower::util::ServiceExt;

    let req = axum::http::Request::builder()
        .uri(path)
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes)
        .unwrap_or_else(|e| panic!("JSON parse error on {}: {} — body: {:?}", path, e, body_bytes));
    (status, json)
}

/// Assert that a JSON value is a structured error with correlation_id, category, and message.
fn assert_structured_error(body: &serde_json::Value) {
    assert!(
        body.get("correlation_id").and_then(|v| v.as_str()).is_some(),
        "error must have correlation_id"
    );
    assert!(body.get("category").and_then(|v| v.as_str()).is_some(), "error must have category");
    assert!(body.get("message").and_then(|v| v.as_str()).is_some(), "error must have message");
    let cid = body["correlation_id"].as_str().unwrap();
    assert_eq!(cid.len(), 36, "correlation_id must be a UUID string");
}

// ── Tests ──────────────────────────────────────────────────────────

/// `GET /api/status` returns 200 with uptime, connections, URB throughput, error count.
#[tokio::test]
async fn test_get_status_returns_200() {
    let app = test_app();
    let (status, body) = get_json(app, "/api/status").await;

    assert_eq!(status, StatusCode::OK);

    // Verify shape
    assert!(
        body.get("uptime_seconds").and_then(|v| v.as_f64()).is_some(),
        "uptime_seconds must be present and numeric"
    );
    assert_eq!(body["active_connections"], serde_json::json!(0));
    assert_eq!(body["urb_throughput"], serde_json::json!(0));
    assert_eq!(body["error_count"], serde_json::json!(0));
    assert!(body.get("status").and_then(|v| v.as_str()) == Some("running"));
}

/// `GET /api/devices` returns 200 with device list.
#[tokio::test]
async fn test_get_devices_returns_200() {
    let app = test_app();
    let (status, body) = get_json(app, "/api/devices").await;

    assert_eq!(status, StatusCode::OK);

    let devices = body.as_array().expect("/api/devices should return an array");
    assert_eq!(devices.len(), 2, "should have 2 mock devices");

    let first = &devices[0];
    assert_eq!(first["vid"], 0x046d);
    assert_eq!(first["pid"], 0xc261);
    assert_eq!(first["busid"], "1-1");
    assert!(first.get("status").and_then(|v| v.as_str()) == Some("available"));

    let second = &devices[1];
    assert_eq!(second["vid"], 0x8087);
    assert_eq!(second["pid"], 0x0024);
    assert_eq!(second["busid"], "1-2");
}

/// `GET /api/config` returns 200 with redacted configuration.
#[tokio::test]
async fn test_get_config_returns_200() {
    let app = test_app();
    let (status, body) = get_json(app, "/api/config").await;

    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["bind_address"], "0.0.0.0");
    assert_eq!(body["port"], 3240);
    assert_eq!(body["encryption_enabled"], false);
}

/// `POST /api/connect` returns 501 with structured error (not yet implemented).
#[tokio::test]
async fn test_post_connect_returns_501() {
    let app = test_app();
    let (status, body) = post_json(
        app,
        "/api/connect",
        &serde_json::json!({
            "host": "192.168.1.100",
            "port": 3240,
            "busid": "1-1"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_structured_error(&body);
    assert!(body["message"].as_str().unwrap().contains("not yet implemented"));
}

/// `POST /api/disconnect` returns 501 with structured error (not yet implemented).
#[tokio::test]
async fn test_post_disconnect_returns_501() {
    let app = test_app();
    let (status, body) = post_json(
        app,
        "/api/disconnect",
        &serde_json::json!({
            "busid": "1-1"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_structured_error(&body);
    assert!(body["message"].as_str().unwrap().contains("not yet implemented"));
}

/// All error responses should include correlation_id, category, and message.
#[tokio::test]
async fn test_unknown_route_returns_404() {
    use axum::body::Body;
    use tower::util::ServiceExt;

    let app = test_app();
    let req = axum::http::Request::builder()
        .uri("/api/nonexistent")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

/// `GET /api/openapi.json` returns 200 with valid OpenAPI spec.
#[tokio::test]
async fn test_get_openapi_returns_200() {
    let app = test_app();
    let (status, body) = get_json(app, "/api/openapi.json").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["openapi"], "3.1.0", "must be OpenAPI 3.1.0");
    assert_eq!(body["info"]["title"], "AnyPlug USB/IP Server API");
    assert!(body["paths"].is_object(), "paths must be present");
    assert!(body["paths"].get("/api/status").is_some(), "status endpoint must be documented");
    assert!(body["paths"].get("/api/devices").is_some(), "devices endpoint must be documented");
    assert!(body["paths"].get("/api/events").is_some(), "events endpoint must be documented");
}

/// Exported device shows status "exported" and connected_client is set.
#[tokio::test]
async fn test_exported_device_shows_client() {
    use std::net::SocketAddrV4;

    let mut exports = HashMap::new();
    let device = make_device_entry("1-1", 0x046d, 0xc261);
    let addr: SocketAddr = SocketAddr::V4(SocketAddrV4::new([192, 168, 1, 5].into(), 45231));
    exports.insert("1-1".to_string(), (addr, device.clone()));

    let app = test_app_with_exports(exports);
    let (status, body) = get_json(app, "/api/devices").await;

    assert_eq!(status, StatusCode::OK);

    let devices = body.as_array().expect("/api/devices should return an array");
    assert_eq!(devices.len(), 2, "should still list all devices");

    // First device (1-1) should be exported
    let first = &devices[0];
    assert_eq!(first["busid"], "1-1");
    assert_eq!(first["status"], "exported");
    assert_eq!(first["connected_client"], "192.168.1.5:45231");

    // Second device (1-2) should still be available
    let second = &devices[1];
    assert_eq!(second["busid"], "1-2");
    assert_eq!(second["status"], "available");
    assert!(second["connected_client"].is_null());
}
