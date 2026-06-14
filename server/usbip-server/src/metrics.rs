//! Prometheus metrics for the USB/IP server.
//!
//! Defines gauges and counters exposed at the `/metrics` endpoint.
//! All metrics use the global default registry so `prometheus::gather()`
//! collects everything automatically.

use std::sync::LazyLock;

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Router};
use prometheus::{
    register_int_counter, register_int_gauge, Encoder, IntCounter, IntGauge, TextEncoder,
};

// ── Metric definitions (lazily registered once) ─────────────────────

/// Number of devices currently exported (gauge).
pub static DEVICES_EXPORTED: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!(
        "usbip_devices_exported",
        "Number of USB devices currently exported"
    )
    .expect("metric registration failed")
});

/// Number of active TCP client connections (gauge).
pub static CLIENTS_CONNECTED: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!(
        "usbip_clients_connected",
        "Number of active TCP client connections"
    )
    .expect("metric registration failed")
});

/// Total number of URB submissions processed (counter).
pub static URB_SUBMIT_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    register_int_counter!(
        "usbip_urb_submit_total",
        "Total number of URB submissions processed"
    )
    .expect("metric registration failed")
});

/// Total bytes transferred in URB payloads (counter).
pub static URB_BYTES_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    register_int_counter!(
        "usbip_urb_bytes_total",
        "Total bytes transferred in URB payloads"
    )
    .expect("metric registration failed")
});

/// Whether encryption is enabled; 1 = enabled, 0 = disabled (gauge).
pub static ENCRYPTION_ENABLED: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!(
        "usbip_encryption_enabled",
        "Whether AES-256-GCM encryption is enabled (1 = yes, 0 = no)"
    )
    .expect("metric registration failed")
});

// ── Router ──────────────────────────────────────────────────────────

/// Shared state for the metrics endpoint (empty, but kept for
/// future extensibility and consistency with other handlers).
#[derive(Clone, Default)]
pub struct MetricsState;

/// Build a router serving the `/metrics` endpoint on a standalone port.
pub fn build_metrics_router() -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(MetricsState)
}

/// `GET /metrics` — Prometheus text-format metrics.
async fn metrics_handler(State(_state): State<MetricsState>) -> impl IntoResponse {
    let metric_families = prometheus::gather();
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    match encoder.encode(&metric_families, &mut buffer) {
        Ok(()) => (
            StatusCode::OK,
            [("Content-Type", "text/plain; charset=utf-8")],
            String::from_utf8(buffer).unwrap_or_default(),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("Content-Type", "text/plain; charset=utf-8")],
            format!("encoding error: {e}"),
        ),
    }
}
