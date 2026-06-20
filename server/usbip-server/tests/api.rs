//! Integration tests for the REST API endpoints added in issue #21.
//!
//! Covers:
//! - `GET  /api/status`   (real uptime)
//! - `GET  /api/devices`  (live device list from `Server::with_backend`)
//! - `POST /api/scan`     (mDNS browse via injected `MdnsBrowser`)
//!
//! Each test exercises the public axum router surface using a
//! `FakeMdnsBrowser` and `DeviceLister` so we never touch real hardware
//! or the network.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::http::StatusCode;
use tokio::sync::Mutex;
use tower::util::ServiceExt;

use usbip_core::protocol::{UsbIpDeviceEntry, U16BE, U32BE};
use usbip_server::api::{self, DeviceLister, DiscoveredServer, MdnsBrowser, RemoteImporter};

/// Serialise env-var manipulation across tests so they don't trip each other up.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// ── Fakes ───────────────────────────────────────────────────────────

/// Fake mDNS browser that returns a canned list of servers.
struct FakeBrowser {
    servers: Vec<DiscoveredServer>,
    last_timeout: std::sync::Mutex<Option<u32>>,
}

impl FakeBrowser {
    fn new(servers: Vec<DiscoveredServer>) -> Self {
        Self { servers, last_timeout: std::sync::Mutex::new(None) }
    }
    fn last_timeout(&self) -> Option<u32> {
        *self.last_timeout.lock().unwrap()
    }
}

impl MdnsBrowser for FakeBrowser {
    fn browse(&self, timeout_secs: u32) -> Vec<DiscoveredServer> {
        *self.last_timeout.lock().unwrap() = Some(timeout_secs);
        self.servers.clone()
    }
}

/// Device lister backed by a fixed list (mirrors the integration with
/// `Server::with_backend(FakeBackend)`).
struct FixedDevices(Vec<UsbIpDeviceEntry>);

impl DeviceLister for FixedDevices {
    fn list_devices(&self) -> Vec<UsbIpDeviceEntry> {
        self.0.clone()
    }
}

/// Fake `RemoteImporter` that records calls and returns a fixed entry.
struct FakeImporter {
    entry: UsbIpDeviceEntry,
    calls: std::sync::Mutex<Vec<(String, u16, String)>>,
}

impl FakeImporter {
    fn new(entry: UsbIpDeviceEntry) -> Self {
        Self { entry, calls: std::sync::Mutex::new(Vec::new()) }
    }
    fn calls(&self) -> Vec<(String, u16, String)> {
        self.calls.lock().unwrap().clone()
    }
}

impl RemoteImporter for FakeImporter {
    fn import(
        &self,
        host: &str,
        port: u16,
        busid: &str,
    ) -> Result<UsbIpDeviceEntry, usbip_core::error::UsbIpError> {
        self.calls.lock().unwrap().push((host.to_string(), port, busid.to_string()));
        Ok(self.entry.clone())
    }
    fn abort(&self, _busid: &str) {}
}

/// Fake importer that always fails — used to exercise error paths.
struct FailingImporter;

impl RemoteImporter for FailingImporter {
    fn import(
        &self,
        _host: &str,
        _port: u16,
        _busid: &str,
    ) -> Result<UsbIpDeviceEntry, usbip_core::error::UsbIpError> {
        Err(usbip_core::error::UsbIpError::from(usbip_core::error::ErrorKind::DeviceNotFound(
            "simulated".into(),
        )))
    }
    fn abort(&self, _busid: &str) {}
}

fn make_entry(busid: &str, vid: u16, pid: u16) -> UsbIpDeviceEntry {
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

// ── App builders ────────────────────────────────────────────────────

fn test_app_with_browser(browser: Arc<dyn MdnsBrowser + Send + Sync>) -> axum::Router {
    test_app_with_browser_and_lister_and_importer(
        browser,
        Arc::new(FixedDevices(vec![make_entry("1-1", 0x046d, 0xc261)])),
        Arc::new(FailingImporter),
    )
}

fn test_app_with_browser_and_lister_and_importer(
    browser: Arc<dyn MdnsBrowser + Send + Sync>,
    lister: Arc<dyn DeviceLister + Send + Sync>,
    importer: Arc<dyn api::RemoteImporter + Send + Sync>,
) -> axum::Router {
    let state = api::AppState {
        start_time: std::time::Instant::now(),
        exports: Arc::new(Mutex::new(HashMap::<String, (SocketAddr, UsbIpDeviceEntry)>::new())),
        device_lister: lister,
        mdns_browser: browser,
        remote_importer: importer,
        config: Arc::new(tokio::sync::RwLock::new(api::ApiConfig {
            bind_address: "0.0.0.0".to_string(),
            port: 3240,
            api_port: 3241,
            encryption_enabled: false,
        })),
        latency_tx: api::new_latency_sender(),
    };
    api::build_router(Arc::new(state))
}

async fn post_json(
    router: axum::Router,
    path: &str,
    body: &serde_json::Value,
) -> (StatusCode, serde_json::Value) {
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
        .unwrap_or_else(|e| panic!("JSON parse error: {} — body: {:?}", e, body_bytes));
    (status, json)
}

async fn put_json(
    router: axum::Router,
    path: &str,
    body: &serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let req = axum::http::Request::builder()
        .uri(path)
        .method("PUT")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes)
        .unwrap_or_else(|e| panic!("JSON parse error: {} — body: {:?}", e, body_bytes));
    (status, json)
}

async fn get_json(router: axum::Router, path: &str) -> (StatusCode, serde_json::Value) {
    let req = axum::http::Request::builder().uri(path).method("GET").body(Body::empty()).unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes)
        .unwrap_or_else(|e| panic!("JSON parse error: {} — body: {:?}", e, body_bytes));
    (status, json)
}

// ── Tests ───────────────────────────────────────────────────────────

/// `POST /api/scan` returns hosts discovered by the injected `MdnsBrowser`.
#[tokio::test]
async fn scan_returns_hosts_from_browser() {
    let browser: Arc<dyn MdnsBrowser + Send + Sync> = Arc::new(FakeBrowser::new(vec![
        DiscoveredServer {
            host: "192.168.1.5".to_string(),
            port: 3240,
            txt: HashMap::from([
                ("version".to_string(), "1.1.1".to_string()),
                ("platform".to_string(), "linux".to_string()),
            ]),
        },
        DiscoveredServer {
            host: "192.168.1.6".to_string(),
            port: 3240,
            txt: HashMap::from([("version".to_string(), "1.1.1".to_string())]),
        },
    ]));
    let app = test_app_with_browser(browser);

    let (status, body) = post_json(app, "/api/scan", &serde_json::json!({})).await;

    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().expect("/api/scan must return an array");
    assert_eq!(arr.len(), 2, "should return both discovered servers");
    assert_eq!(arr[0]["host"], "192.168.1.5");
    assert_eq!(arr[0]["port"], 3240);
    assert_eq!(arr[0]["txt"]["version"], "1.1.1");
    assert_eq!(arr[0]["txt"]["platform"], "linux");
    assert_eq!(arr[1]["host"], "192.168.1.6");
}

/// `POST /api/scan` with no `timeout_secs` defaults to 5 seconds.
#[tokio::test]
async fn scan_default_timeout_is_five() {
    let browser = Arc::new(FakeBrowser::new(vec![]));
    let app = test_app_with_browser(browser.clone());

    let _ = post_json(app, "/api/scan", &serde_json::json!({})).await;

    assert_eq!(browser.last_timeout(), Some(5));
}

/// `POST /api/scan` clamps `timeout_secs` to a maximum of 30.
#[tokio::test]
async fn scan_clamps_timeout_to_max_thirty() {
    let browser = Arc::new(FakeBrowser::new(vec![]));
    let app = test_app_with_browser(browser.clone());

    let _ = post_json(app, "/api/scan", &serde_json::json!({"timeout_secs": 999})).await;

    assert_eq!(browser.last_timeout(), Some(30));
}

/// `POST /api/scan` passes through a valid `timeout_secs`.
#[tokio::test]
async fn scan_passes_through_valid_timeout() {
    let browser = Arc::new(FakeBrowser::new(vec![]));
    let app = test_app_with_browser(browser.clone());

    let _ = post_json(app, "/api/scan", &serde_json::json!({"timeout_secs": 12})).await;

    assert_eq!(browser.last_timeout(), Some(12));
}

/// `POST /api/scan` clamps `timeout_secs` to a minimum of 1.
#[tokio::test]
async fn scan_clamps_timeout_to_min_one() {
    let browser = Arc::new(FakeBrowser::new(vec![]));
    let app = test_app_with_browser(browser.clone());

    let _ = post_json(app, "/api/scan", &serde_json::json!({"timeout_secs": 0})).await;

    assert_eq!(browser.last_timeout(), Some(1));
}

/// `Server::with_backend(FakeBackend)` builds an `AppState` whose
/// `device_lister` reflects the backend's devices — verifying the
/// production wiring (no mock path).
#[tokio::test]
async fn devices_uses_lister_from_server_with_backend() {
    use usbip_server::usb_backend::{make_test_entry, FakeBackend};
    use usbip_server::Server;
    use usbip_server::ServerConfig;

    let backend = FakeBackend::new(vec![
        make_test_entry("1-1", 0x046d, 0xc261),
        make_test_entry("1-2", 0x8087, 0x0024),
    ]);
    let server = Server::with_backend(ServerConfig::default(), Box::new(backend)).await.unwrap();
    let state = server.app_state(3241);

    let router = api::build_router(Arc::new(state));
    let (status, body) = get_json(router, "/api/devices").await;

    assert_eq!(status, StatusCode::OK);
    let devices = body.as_array().expect("/api/devices must be an array");
    assert_eq!(devices.len(), 2);
    let busids: Vec<&str> = devices.iter().map(|d| d["busid"].as_str().unwrap()).collect();
    assert!(busids.contains(&"1-1"));
    assert!(busids.contains(&"1-2"));
}

/// `GET /api/status` reflects a non-zero, growing uptime based on
/// `AppState.start_time`. This pins the behaviour that uptime is
/// computed from the real start instant, not a hardcoded value.
#[tokio::test]
async fn status_uptime_advances_from_start_time() {
    let browser = Arc::new(FakeBrowser::new(vec![]));
    let app = test_app_with_browser(browser);

    let (status, body) = get_json(app.clone(), "/api/status").await;
    assert_eq!(status, StatusCode::OK);
    let first = body["uptime_seconds"].as_f64().expect("uptime_seconds is a float");
    assert!(first >= 0.0, "uptime must be non-negative");

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let (status2, body2) = get_json(app, "/api/status").await;
    assert_eq!(status2, StatusCode::OK);
    let second = body2["uptime_seconds"].as_f64().expect("uptime_seconds is a float");
    assert!(second > first + 0.1, "uptime must advance (first={first}, second={second})");
}

/// `POST /api/connect` returns 200 with `{busid, vid, pid, status}` from the
/// injected `RemoteImporter`, and stores the entry in AppState so a follow-up
/// `GET /api/devices` shows it as exported.
#[tokio::test]
async fn connect_returns_imported_device_info() {
    let browser = Arc::new(FakeBrowser::new(vec![]));
    let importer = Arc::new(FakeImporter::new(make_entry("2-3", 0x1234, 0x5678)));

    let router = test_app_with_browser_and_lister_and_importer(
        browser,
        Arc::new(FixedDevices(vec![make_entry("1-1", 0x046d, 0xc261)])),
        importer.clone(),
    );

    let (status, body) = post_json(
        router,
        "/api/connect",
        &serde_json::json!({"host": "10.0.0.5", "port": 3240, "busid": "2-3"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["busid"], "2-3");
    assert_eq!(body["vid"], 0x1234);
    assert_eq!(body["pid"], 0x5678);
    assert_eq!(body["status"], "imported");

    let calls = importer.calls();
    assert_eq!(calls.len(), 1, "importer should be called exactly once");
    assert_eq!(calls[0].0, "10.0.0.5");
    assert_eq!(calls[0].1, 3240);
    assert_eq!(calls[0].2, "2-3");
}

/// `POST /api/connect` returns a structured error (with `correlation_id`)
/// when the injected `RemoteImporter` fails.
#[tokio::test]
async fn connect_returns_structured_error_on_importer_failure() {
    let browser = Arc::new(FakeBrowser::new(vec![]));
    let router = test_app_with_browser_and_lister_and_importer(
        browser,
        Arc::new(FixedDevices(vec![make_entry("1-1", 0x046d, 0xc261)])),
        Arc::new(FailingImporter),
    );

    let (status, body) = post_json(
        router,
        "/api/connect",
        &serde_json::json!({"host": "10.0.0.5", "port": 3240, "busid": "9-9"}),
    )
    .await;

    assert!(status.is_server_error(), "expected a 5xx failure, got {}", status);
    let cid = body["correlation_id"].as_str().expect("correlation_id");
    assert_eq!(cid.len(), 36, "correlation_id must be a UUID string");
    assert!(body["category"].is_string(), "category must be present");
    assert!(body["message"].is_string(), "message must be present");
}

/// Connect → device is exported → disconnect → device is gone → re-connect works.
#[tokio::test]
async fn disconnect_then_reconnect_round_trip() {
    let browser = Arc::new(FakeBrowser::new(vec![]));
    let importer = Arc::new(FakeImporter::new(make_entry("2-3", 0x1234, 0x5678)));
    let exports: Arc<Mutex<HashMap<String, (SocketAddr, UsbIpDeviceEntry)>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let state = Arc::new(api::AppState {
        start_time: std::time::Instant::now(),
        exports: exports.clone(),
        device_lister: Arc::new(FixedDevices(vec![make_entry("1-1", 0x046d, 0xc261)])),
        mdns_browser: browser,
        remote_importer: importer.clone(),
        config: Arc::new(tokio::sync::RwLock::new(api::ApiConfig {
            bind_address: "0.0.0.0".to_string(),
            port: 3240,
            api_port: 3241,
            encryption_enabled: false,
        })),
        latency_tx: api::new_latency_sender(),
    });

    // 1. First connect: imported.
    let (status, body) = post_json(
        api::build_router(state.clone()),
        "/api/connect",
        &serde_json::json!({"host": "10.0.0.5", "port": 3240, "busid": "2-3"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "imported");
    assert_eq!(exports.lock().await.len(), 1, "device should be exported");

    // 2. Disconnect: removes the entry.
    let (status, body) = post_json(
        api::build_router(state.clone()),
        "/api/disconnect",
        &serde_json::json!({"busid": "2-3"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "disconnected");
    assert_eq!(exports.lock().await.len(), 0, "exports should be empty after disconnect");

    // 3. Re-connect the same busid: must succeed (not "already exported").
    let (status, body) = post_json(
        api::build_router(state),
        "/api/connect",
        &serde_json::json!({"host": "10.0.0.5", "port": 3240, "busid": "2-3"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "imported");

    assert_eq!(
        importer.calls().len(),
        2,
        "importer should have been called twice (connect + reconnect)"
    );
}

/// `PUT /api/config` rejects out-of-range ports (0 and >65535) with a
/// 400 + structured `ApiErrorResponse`.
#[tokio::test]
async fn put_config_rejects_out_of_range_ports() {
    let browser = Arc::new(FakeBrowser::new(vec![]));
    let router = test_app_with_browser(browser);

    let (status, body) = put_json(
        router,
        "/api/config",
        &serde_json::json!({"bind_address": "0.0.0.0", "port": 0, "api_port": 3241}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "port=0 must be rejected (got body={body})");
    assert!(body["correlation_id"].is_string(), "structured error must have correlation_id");
    assert!(body["category"].is_string(), "category must be present");
    assert!(body["message"].is_string(), "message must be present");

    let router = test_app_with_browser(Arc::new(FakeBrowser::new(vec![])));
    let (status, body) = put_json(
        router,
        "/api/config",
        &serde_json::json!({"bind_address": "0.0.0.0", "port": 70000, "api_port": 3241}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "port=70000 must be rejected (got body={body})");
    assert!(body["correlation_id"].is_string());
}

/// `PUT /api/config` writes a TOML file that `load_config()` re-reads on
/// the next process — round-trip persistence.
#[tokio::test]
async fn put_config_persists_and_reloads() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg_path = tmp.path().join("server.toml");

    let _guard = ENV_LOCK.lock().unwrap();
    unsafe { std::env::set_var("ANYPLUG_CONFIG_PATH", &cfg_path) };

    let browser = Arc::new(FakeBrowser::new(vec![]));
    let router = test_app_with_browser(browser);

    let (status, _body) = put_json(
        router,
        "/api/config",
        &serde_json::json!({"bind_address": "127.0.0.1", "port": 5555, "api_port": 6666, "encryption_enabled": true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let text = std::fs::read_to_string(&cfg_path).expect("config file written");
    assert!(text.contains("5555"), "port must be in the TOML file: {text}");
    assert!(text.contains("6666"), "api_port must be in the TOML file: {text}");
    assert!(text.contains("127.0.0.1"), "bind_address must be in the TOML file: {text}");
    assert!(text.contains("true"), "encryption_enabled must be in the TOML file: {text}");

    let loaded = api::load_config();
    assert_eq!(loaded.port, 5555);
    assert_eq!(loaded.api_port, 6666);
    assert_eq!(loaded.bind_address, "127.0.0.1");
    assert!(loaded.encryption_enabled);

    unsafe { std::env::remove_var("ANYPLUG_CONFIG_PATH") };
}

/// `/api/events` WS clients receive `latency` text frames broadcast on
/// the `AppState` channel.
#[tokio::test]
async fn ws_events_forwards_latency_frames() {
    use futures::StreamExt;
    use tokio_tungstenite::tungstenite::Message as WsMsg;

    let browser = Arc::new(FakeBrowser::new(vec![]));
    let latency_tx = api::new_latency_sender();

    let state = Arc::new(api::AppState {
        start_time: std::time::Instant::now(),
        exports: Arc::new(Mutex::new(HashMap::new())),
        device_lister: Arc::new(FixedDevices(vec![make_entry("1-1", 0x046d, 0xc261)])),
        mdns_browser: browser,
        remote_importer: Arc::new(FailingImporter),
        config: Arc::new(tokio::sync::RwLock::new(api::ApiConfig::default())),
        latency_tx: latency_tx.clone(),
    });

    let app = api::build_router(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Connect a WS client.
    let url = format!("ws://{}/api/events", addr);
    let (mut ws, _resp) = tokio_tungstenite::connect_async(&url).await.expect("ws connect");

    // Drain the initial `connected` frame.
    let _ = ws.next().await.expect("first frame").expect("first frame ok");

    // Broadcast a latency sample and expect a text frame back.
    latency_tx
        .send(api::LatencySample { latency_us: 750, device: "1-1".to_string(), seqnum: 42 })
        .expect("broadcast send");

    let frame = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
        .await
        .expect("timeout waiting for frame")
        .expect("ws stream")
        .expect("ws message");
    let text = match frame {
        WsMsg::Text(t) => t,
        other => panic!("expected text frame, got {other:?}"),
    };
    let json: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(json["type"], "latency");
    assert_eq!(json["payload"]["latency_us"], 750);
    assert_eq!(json["payload"]["device"], "1-1");
    assert_eq!(json["payload"]["seqnum"], 42);

    let _ = ws.close(None).await;
    let _ = ws_stream_send_close(ws);
    server.abort();
}

async fn ws_stream_send_close<S>(_ws: S) {}
