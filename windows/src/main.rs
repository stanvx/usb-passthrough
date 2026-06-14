//! AnyPlug — Windows application.
//!
//! Entry point for the CLI (`serve`, `connect`) and GUI (`gui`) modes.
//! When run without a subcommand, defaults to launching the system tray GUI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(unused_imports)] // workspace deps imported for future use

#[macro_use]
extern crate windows_service;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use usbip_core::error::UsbIpError;

mod windows_usb;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "anyplug")]
#[command(about = "AnyPlug — Share or use USB devices over the network")]
#[command(long_about = "\
AnyPlug Windows application for USB/IP device sharing.\n\
\n  serve   — Start the USB/IP server to export local USB devices\n\
  connect — Connect to a remote USB/IP server and import a device\n\
  gui     — Launch the system-tray GUI (default)")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to configuration file
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the USB/IP server — exports local USB devices to the network
    #[command(aliases = ["s"])]
    Serve {
        /// Bind address (default: 0.0.0.0)
        #[arg(short, long, default_value = "0.0.0.0")]
        bind: String,

        /// TCP port (default: 3240)
        #[arg(short, long, default_value_t = 3240)]
        port: u16,

        /// Only allow these VID:PID pairs (e.g. 046d:c261)
        #[arg(long = "allow", value_parser = parse_vid_pid)]
        allowed: Vec<(u16, u16)>,

        /// Disable interactive confirmation for new connections
        #[arg(long)]
        no_confirm: bool,

        /// Enable AES-256-GCM encryption
        #[arg(long)]
        encrypt: bool,

        /// Run as a background Windows service
        #[arg(long)]
        service: bool,
    },

    /// Connect to a remote USB/IP server and import a device
    #[command(aliases = ["c"])]
    Connect {
        /// Server address (host:port)
        server: String,

        /// Bus-ID of the device to import (e.g. "1-2.3")
        #[arg(short, long)]
        busid: Option<String>,

        /// Automatically reconnect on disconnect
        #[arg(long)]
        auto_reconnect: bool,

        /// Use mDNS to discover servers instead of a direct address
        #[arg(long)]
        discover: bool,
    },

    /// Launch the system-tray GUI
    #[command(aliases = ["g"])]
    Gui,
}

fn parse_vid_pid(s: &str) -> Result<(u16, u16), String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err("format: VID:PID (e.g., 046d:c261)".to_string());
    }
    let vid = u16::from_str_radix(parts[0], 16).map_err(|e| e.to_string())?;
    let pid = u16::from_str_radix(parts[1], 16).map_err(|e| e.to_string())?;
    Ok((vid, pid))
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer().with_target(true))
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cli = Cli::parse();

    let result = match cli.command.unwrap_or(Commands::Gui) {
        Commands::Serve { bind, port, allowed, no_confirm, encrypt, service } => {
            if service {
                run_as_service()
            } else {
                run_server(bind, port, allowed, !no_confirm, encrypt)
            }
        },
        Commands::Connect { server, busid, auto_reconnect, discover } => {
            run_client(server, busid, auto_reconnect, discover)
        },
        Commands::Gui => run_gui(),
    };

    if let Err(e) = result {
        // Try to extract structured UsbIpError for rich diagnostics
        if let Some(usbip_err) = e.downcast_ref::<UsbIpError>() {
            eprintln!(
                "Fatal: [corr={}] [cat={}] {}",
                usbip_err.correlation_id(),
                usbip_err.category(),
                usbip_err
            );
        } else {
            eprintln!("Fatal: {e}");
        }
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

fn run_server(
    bind: String,
    port: u16,
    allowed: Vec<(u16, u16)>,
    require_confirmation: bool,
    encryption_enabled: bool,
) -> anyhow::Result<()> {
    tracing::info!("Starting USB/IP server on {bind}:{port}");

    let config = usbip_server::ServerConfig {
        bind_address: bind,
        port,
        allowed_vid_pid: allowed,
        require_confirmation,
        encryption_enabled,
        tcp_nodelay: true,
    };

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let server = usbip_server::Server::new(config).await?;
        server.run().await
    })
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

fn run_client(
    server: String,
    busid: Option<String>,
    auto_reconnect: bool,
    discover: bool,
) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;

    if discover {
        tracing::info!("Discovering USB/IP servers via mDNS...");
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let client = usbip_client::Client::new(usbip_client::ClientConfig {
                server_addr: None,
                use_mdns: true,
                auto_reconnect,
                reconnect_attempts: if auto_reconnect { 10 } else { 0 },
                reconnect_delay_ms: 2000,
                tcp_nodelay: true,
            })?;

            let servers = client.discover_servers().await?;
            if servers.is_empty() {
                anyhow::bail!("No USB/IP servers discovered on the network");
            }

            println!("Discovered servers:");
            for (i, addr) in servers.iter().enumerate() {
                println!("  {}. {addr}", i + 1);
            }

            // Auto-connect to the first server
            let addr = servers[0];
            let devices = client.list_remote_devices(addr).await?;
            println!("\nDevices on {addr}:");
            for dev in &devices {
                println!(
                    "  {:04x}:{:04x}  {}  {}  speed={}",
                    dev.vid(),
                    dev.pid(),
                    dev.busid_str(),
                    dev.path_str(),
                    dev.speed_val(),
                );
            }

            // Import the first device if busid not specified
            let target_busid = busid.unwrap_or_else(|| devices[0].busid_str().to_string());
            let _imported = client.import_device(addr, &target_busid).await?;
            println!("Imported device {target_busid} from {addr}");

            // Keep alive
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
            }
        })
    } else {
        let addr: std::net::SocketAddr =
            server.parse().map_err(|_| anyhow::anyhow!("Invalid server address: {server}"))?;

        tracing::info!("Connecting to USB/IP server at {addr}");

        rt.block_on(async {
            let client = usbip_client::Client::new(usbip_client::ClientConfig {
                server_addr: Some(addr),
                use_mdns: false,
                auto_reconnect,
                reconnect_attempts: if auto_reconnect { 10 } else { 0 },
                reconnect_delay_ms: 2000,
                tcp_nodelay: true,
            })?;

            let devices = client.list_remote_devices(addr).await?;
            println!("Devices on {addr}:");
            for dev in &devices {
                println!(
                    "  {:04x}:{:04x}  {}  {}  speed={}",
                    dev.vid(),
                    dev.pid(),
                    dev.busid_str(),
                    dev.path_str(),
                    dev.speed_val(),
                );
            }

            let target_busid = busid.unwrap_or_else(|| devices[0].busid_str().to_string());
            let _imported = client.import_device(addr, &target_busid).await?;
            println!("Imported device {target_busid} from {addr}");

            // Keep alive / auto-reconnect
            if auto_reconnect {
                // wait indefinitely — the client handles reconnection internally
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                }
            }

            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// GUI (egui system-tray window)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "nogui"))]
fn run_gui() -> anyhow::Result<()> {
    tracing::info!("Launching AnyPlug GUI");

    let rt = tokio::runtime::Runtime::new()?;
    let _rt = rt.enter();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([480.0, 360.0])
            .with_min_inner_size([320.0, 240.0]),
        ..Default::default()
    };

    let app = AnyPlugApp::default();

    eframe::run_native("AnyPlug", options, Box::new(|_cc| Ok(Box::new(app))))
        .map_err(|e| anyhow::anyhow!("GUI error: {e}"))
}

#[cfg(feature = "nogui")]
fn run_gui() -> anyhow::Result<()> {
    anyhow::bail!("GUI support is disabled (compiled with 'nogui' feature)")
}

// ---------------------------------------------------------------------------
// egui application
// ---------------------------------------------------------------------------

#[derive(Default)]
struct AnyPlugApp {
    /// Local USB devices discovered via SetupAPI
    local_devices: Vec<windows_usb::UsbDeviceInfo>,
    /// Error message to display
    error: Option<String>,
    /// Server status
    server_running: bool,
    /// Server port
    server_port: u16,
}

impl eframe::App for AnyPlugApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Main panel ---
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("AnyPlug");

            ui.separator();

            // Device list section
            ui.label("Local USB Devices:");
            ui.add_space(4.0);

            if self.local_devices.is_empty() {
                ui.label("(No devices detected — click Refresh)");
            } else {
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    for dev in &self.local_devices {
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{:04x}:{:04x}  {}",
                                dev.vendor_id, dev.product_id, dev.description
                            ));
                            if ui.button("Export").clicked() {
                                // TODO: trigger server export for this device
                                tracing::info!("Export requested for {dev:?}");
                            }
                        });
                    }
                });
            }

            ui.add_space(8.0);

            // Action buttons
            ui.horizontal(|ui| {
                if ui.button("🔄 Refresh").clicked() {
                    self.refresh_devices();
                }
                if ui.button("⚙ Start Server").clicked() {
                    self.start_server();
                }
                if ui.button("🔗 Discover").clicked() {
                    // TODO: launch mDNS discovery popup
                    tracing::info!("mDNS discovery requested");
                }
            });

            ui.separator();

            // Status bar
            ui.horizontal(|ui| {
                if self.server_running {
                    ui.label(format!("✅ Server running on port {}", self.server_port));
                } else {
                    ui.label("⏹ Server stopped");
                }
            });

            // Error display
            if let Some(ref err) = self.error {
                ui.separator();
                ui.colored_label(egui::Color32::RED, format!("Error: {err}"));
            }
        });

        // Request repaint continuously for live feel
        ctx.request_repaint();
    }
}

impl AnyPlugApp {
    fn refresh_devices(&mut self) {
        match windows_usb::enumerate_usb_devices() {
            Ok(devices) => {
                tracing::info!("Found {} USB device(s)", devices.len());
                self.local_devices = devices;
                self.error = None;
            },
            Err(e) => {
                tracing::error!("Failed to enumerate USB devices: {e}");
                self.error = Some(format!("Enumeration failed: {e}"));
            },
        }
    }

    fn start_server(&mut self) {
        if self.server_running {
            self.error = Some("Server is already running".to_string());
            return;
        }

        // Start server on a background thread
        let port = self.server_port;
        std::thread::spawn(move || {
            let config = usbip_server::ServerConfig {
                bind_address: "0.0.0.0".to_string(),
                port,
                allowed_vid_pid: Vec::new(),
                require_confirmation: true,
                encryption_enabled: false,
                tcp_nodelay: true,
            };

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                match usbip_server::Server::new(config).await {
                    Ok(server) => {
                        tracing::info!("Server started successfully on port {port}");
                        if let Err(e) = server.run().await {
                            tracing::error!("Server error: {e}");
                        }
                    },
                    Err(e) => {
                        tracing::error!("Failed to create server: {e}");
                    },
                }
            });
        });

        self.server_running = true;
    }
}

// ---------------------------------------------------------------------------
// Windows Service
// ---------------------------------------------------------------------------

fn run_as_service() -> anyhow::Result<()> {
    tracing::info!("Registering AnyPlug as a Windows service...");

    let service_name = "anyplug-service";
    let service_display_name = "AnyPlug Service";
    let service_description = "Shares local USB devices over the network via the USB/IP protocol.";

    windows_service::service_dispatcher::start(service_name, ffi_service_main)
        .map_err(|e| anyhow::anyhow!("Service dispatcher error: {e}"))?;

    Ok(())
}

// Generate the FFI entry point. The `define_windows_service!` macro
// produces a low-level `extern "system" fn` that parses service arguments
// and forwards them to `my_service_main`.
define_windows_service!(ffi_service_main, my_service_main);

fn my_service_main(_arguments: Vec<std::ffi::OsString>) {
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel::<()>();

    let status_handle = service_control_handler::register(
        "anyplug-service",
        move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            },
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        },
    )
    .expect("Failed to register service control handler");

    // Report running
    status_handle
        .set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: std::time::Duration::from_secs(5),
            process_id: Some(std::process::id()),
        })
        .expect("Failed to set service status");

    tracing::info!("AnyPlug service is running");

    // Start the server
    let config = usbip_server::ServerConfig {
        bind_address: "0.0.0.0".to_string(),
        port: 3240,
        allowed_vid_pid: Vec::new(),
        require_confirmation: false,
        encryption_enabled: false,
        tcp_nodelay: true,
    };

    let server_handle = std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
        rt.block_on(async {
            let server =
                usbip_server::Server::new(config).await.expect("Failed to create server");
            server.run().await.ok();
        });
    });

    // Wait for shutdown signal
    let _ = shutdown_rx.recv();

    tracing::info!("Shutting down AnyPlug service");

    status_handle
        .set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: std::time::Duration::default(),
            process_id: None,
        })
        .expect("Failed to set service status");
}
