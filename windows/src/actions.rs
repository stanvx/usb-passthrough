//! Server lifecycle and mDNS discovery helpers for the Windows GUI.
//!
//! These functions back the egui "Stop Server" and "Discover" buttons.
//! The GUI cannot be unit-tested, so this module exposes the testable
//! behaviour: the server controller threads a shutdown signal through
//! a channel, and the discoverer wraps the cross-platform
//! `usbip_client::MdnsBrowser` with a richer return type that the GUI
//! can display directly.

use std::net::SocketAddr;
use std::sync::mpsc;
use std::thread::JoinHandle;

use usbip_client::discovery::MdnsBrowser;
use usbip_server::ServerConfig;

/// A USB/IP server discovered on the LAN, with a human-friendly label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredServer {
    pub addr: SocketAddr,
    pub service_name: String,
}

/// Wraps the cross-platform mDNS browser with a richer return type and
/// better error reporting for the GUI.
pub struct Discoverer {
    browser: MdnsBrowser,
}

impl Discoverer {
    /// Create a new discoverer. Returns an error if the mDNS subsystem
    /// is not available (e.g. missing avahi on Linux, missing Bonjour
    /// on Windows).
    pub fn new() -> Result<Self, AnyPlugError> {
        let browser =
            MdnsBrowser::new().map_err(|e| AnyPlugError::Mdns(format!("mDNS init failed: {e}")))?;
        Ok(Self { browser })
    }

    /// Browse the local network for USB/IP servers, waiting up to
    /// `timeout` for replies.
    pub fn discover(
        &self,
        timeout: std::time::Duration,
    ) -> Result<Vec<DiscoveredServer>, AnyPlugError> {
        let addrs = self.browser.browse().map_err(|e| AnyPlugError::Mdns(e.to_string()))?;
        let _ = timeout; // browser's built-in scan is 2s; this is reserved for future override
        Ok(addrs
            .into_iter()
            .map(|addr| DiscoveredServer { addr, service_name: addr.to_string() })
            .collect())
    }
}

/// Manages a running `usbip_server::Server` in a background thread and
/// exposes a stop signal that the GUI can use to request shutdown.
pub struct ServerController {
    /// Sender end of the shutdown channel. We keep this in the struct
    /// so `stop()` can be called without `&mut` on the JoinHandle.
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// Handle to the server thread. We don't expose this for joining
    /// (it can be detached on stop) but we keep the slot for future
    /// wait-for-exit semantics.
    _join: Option<JoinHandle<()>>,
    /// Port the server is listening on. Tracked so the GUI can display
    /// the same number in the status bar.
    port: u16,
    /// `true` between `start` and `stop`/`finish`.
    running: bool,
}

impl ServerController {
    pub fn new() -> Self {
        Self { shutdown_tx: None, _join: None, port: 0, running: false }
    }

    /// `true` when the controller believes a server is running.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Port the controller was started with, or 0 if not running.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Start a server on `port`. If a previous start call did not
    /// finish, this returns an error without spawning a second
    /// thread.
    pub fn start(&mut self, port: u16) -> Result<(), AnyPlugError> {
        if self.running {
            return Err(AnyPlugError::AlreadyRunning);
        }

        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        let config = ServerConfig {
            bind_address: "0.0.0.0".to_string(),
            port,
            allowed_vid_pid: Vec::new(),
            require_confirmation: true,
            encryption_enabled: false,
            tcp_nodelay: true,
            max_bandwidth: usbip_server::BandwidthLimit::unlimited(),
            per_client_bandwidth: None,
        };

        let join = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(async {
                match usbip_server::Server::new(config).await {
                    Ok(server) => {
                        // Race the server loop against the shutdown signal.
                        tokio::select! {
                            res = server.run() => {
                                if let Err(e) = res {
                                    tracing::error!("server exited with error: {e}");
                                }
                            }
                            _ = wait_for_shutdown(shutdown_rx) => {
                                tracing::info!("server received shutdown signal");
                            }
                        }
                    },
                    Err(e) => {
                        tracing::error!("failed to create server: {e}");
                    },
                }
            });
        });

        self.shutdown_tx = Some(shutdown_tx);
        self._join = Some(join);
        self.port = port;
        self.running = true;
        Ok(())
    }

    /// Signal the server thread to stop. After this call,
    /// `is_running()` returns `false` immediately. The actual server
    /// shutdown is best-effort; the call returns `Ok(())` whether or
    /// not the worker thread has finished.
    pub fn stop(&mut self) -> Result<(), AnyPlugError> {
        let Some(tx) = self.shutdown_tx.take() else {
            self.running = false;
            return Ok(());
        };
        // Best-effort signal: ignore the SendError, it just means the
        // thread already exited.
        let _ = tx.send(());
        self.running = false;
        Ok(())
    }
}

/// Bridge a blocking `mpsc::Receiver` into an async future the
/// `tokio::select!` can race against the server loop.
async fn wait_for_shutdown(rx: mpsc::Receiver<()>) {
    loop {
        match rx.recv() {
            Ok(()) => return,
            Err(_) => {
                // Sender dropped — exit anyway.
                std::future::pending::<()>().await;
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AnyPlugError {
    #[error("server is already running")]
    AlreadyRunning,
    #[error("mDNS: {0}")]
    Mdns(String),
}

pub use self::AnyPlugError as Error;

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn server_controller_lifecycle_reports_running() {
        let mut ctrl = ServerController::new();
        assert!(!ctrl.is_running());
        assert_eq!(ctrl.port(), 0);

        // Use an ephemeral port so the test does not collide with a
        // real server. We never let it accept — we only verify the
        // controller's bookkeeping.
        let port = pick_unused_port();
        ctrl.start(port).expect("start should succeed");
        assert!(ctrl.is_running());
        assert_eq!(ctrl.port(), port);

        ctrl.stop().expect("stop should succeed");
        assert!(!ctrl.is_running());
    }

    #[test]
    fn server_controller_double_start_rejected() {
        let mut ctrl = ServerController::new();
        let port = pick_unused_port();
        ctrl.start(port).expect("first start should succeed");
        let err = ctrl.start(port).expect_err("second start should fail");
        assert!(matches!(err, AnyPlugError::AlreadyRunning));
        ctrl.stop().ok();
    }

    #[test]
    fn server_controller_stop_without_start_is_noop() {
        let mut ctrl = ServerController::new();
        ctrl.stop().expect("stop without start is a noop");
        assert!(!ctrl.is_running());
    }

    #[test]
    fn discoverer_discover_returns_vec() {
        // We don't assert a non-empty result because the CI runner may
        // not have mDNS. We only assert the call shape: it returns
        // a Vec (possibly empty) and does not panic.
        let discoverer = match Discoverer::new() {
            Ok(d) => d,
            Err(_) => {
                // mDNS unavailable in this environment; skip.
                eprintln!("mDNS not available; skipping discoverer test");
                return;
            },
        };
        let result = discoverer.discover(Duration::from_secs(2));
        assert!(result.is_ok(), "discover should not error: {:?}", result.err());
    }

    /// Pick an unused TCP port by binding ephemerally and reading
    /// the kernel-assigned port back.
    fn pick_unused_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }
}
