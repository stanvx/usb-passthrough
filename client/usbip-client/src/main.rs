//! USB/IP Client — CLI entry point.
//!
//! Supports both foreground connection mode and a persistent daemon mode
//! with Unix domain socket control.

use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use usbip_client::{Client, ClientConfig};
#[cfg(unix)]
use usbip_client::daemon::{DaemonCommand, DaemonConfig, DaemonManager};
use usbip_core::error::UsbIpResult;

#[derive(Parser)]
#[command(name = "usbip-client")]
#[command(about = "USB/IP Client — Import USB devices over the network")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Discover USB/IP servers on the local network (mDNS)
    Discover,
    /// List devices exported by a server
    List {
        /// Server address (host:port)
        server: SocketAddr,
    },
    /// Import a USB device from a server (foreground mode)
    Connect {
        /// Server address (host:port)
        server: SocketAddr,
        /// Device bus ID to import
        busid: String,
        /// Don't auto-reconnect on disconnect
        #[arg(long)]
        no_reconnect: bool,
    },
    /// Run as persistent background daemon
    #[cfg(unix)]
    Daemon {
        /// Path to config file (default: /etc/usbip-client/config.toml)
        #[arg(long, short)]
        config: Option<PathBuf>,
    },
    /// Control a running daemon via its Unix socket
    #[cfg(unix)]
    #[command(name = "daemon-ctl")]
    DaemonCtl {
        /// Path to the daemon's Unix socket (default: auto-detect)
        #[arg(long, short)]
        socket: Option<PathBuf>,
        #[command(subcommand)]
        action: DaemonCtlAction,
    },
}

#[cfg(unix)]
#[derive(Subcommand)]
enum DaemonCtlAction {
    /// Tell the daemon to connect and import a device
    Connect {
        /// Server address (host:port)
        server: SocketAddr,
        /// Device bus ID to import
        busid: String,
    },
    /// Tell the daemon to disconnect a device
    Disconnect {
        /// Device bus ID to disconnect
        busid: String,
    },
    /// Query daemon status
    Status,
    /// Shut down the daemon gracefully
    Shutdown,
}

#[tokio::main]
async fn main() -> UsbIpResult<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Discover => {
            let config = ClientConfig { use_mdns: true, ..Default::default() };
            let client = Client::new(config)?;
            let servers = client.discover_servers()?;

            if servers.is_empty() {
                println!("No USB/IP servers found on the network.");
            } else {
                println!("USB/IP servers found:");
                for addr in servers {
                    println!("  {}", addr);
                }
            }
        },

        Commands::List { server } => {
            let config =
                ClientConfig { server_addr: Some(server), use_mdns: false, ..Default::default() };
            let client = Client::new(config)?;
            let devices = client.list_remote_devices(server).await?;

            if devices.is_empty() {
                println!("No exportable devices on {}", server);
            } else {
                println!("Devices on {}:", server);
                for dev in devices {
                    println!(
                        "  {:04x}:{:04x}  {}  {}",
                        dev.vid(),
                        dev.pid(),
                        dev.busid_str(),
                        dev.path_str(),
                    );
                }
            }
        },

        Commands::Connect { server, busid, no_reconnect } => {
            let config = ClientConfig {
                server_addr: Some(server),
                use_mdns: false,
                auto_reconnect: !no_reconnect,
                ..Default::default()
            };
            let client = Client::new(config)?;

            println!("Importing device {} from {}...", busid, server);
            let imported = client.import_device(server, &busid).await?;

            println!(
                "Device imported: {:04x}:{:04x}",
                imported.device_entry.vid(),
                imported.device_entry.pid(),
            );
            println!("Press Ctrl+C to detach.");

            // Wait for signal
            tokio::signal::ctrl_c().await?;
            println!("\nDetaching device...");
        },

        #[cfg(unix)]
        Commands::Daemon { config } => {
            // Determine config path
            let config_path = match config {
                Some(path) => path,
                None => {
                    // Try default paths, falling back to first writable candidate
                    match DaemonConfig::find_config() {
                        Some(path) => path,
                        None => PathBuf::from("/etc/usbip-client/config.toml"),
                    }
                },
            };

            let daemon_config = DaemonConfig::load(&config_path)?;
            let manager = DaemonManager::new(daemon_config)?;
            println!("Starting USB/IP client daemon...");
            manager.run().await?;
        },
        #[cfg(unix)]
        Commands::DaemonCtl { socket, action } => {
            // Determine socket path
            let socket_path = match socket {
                Some(path) => path,
                None => {
                    // Try to auto-detect from config
                    match DaemonConfig::find_config() {
                        Some(cfg_path) => DaemonConfig::load(&cfg_path)
                            .map(|cfg| cfg.socket_path)
                            .unwrap_or_else(|_| PathBuf::from("/var/run/usbip-client/usbip-client.sock")),
                        None => PathBuf::from("/var/run/usbip-client/usbip-client.sock"),
                    }
                },
            };

            if !socket_path.exists() {
                eprintln!(
                    "Error: daemon socket not found at {}. Is the daemon running?",
                    socket_path.display()
                );
                std::process::exit(1);
            }

            let cmd = match action {
                DaemonCtlAction::Connect { server, busid } => {
                    DaemonCommand::Connect { server, busid }
                },
                DaemonCtlAction::Disconnect { busid } => {
                    DaemonCommand::Disconnect { busid }
                },
                DaemonCtlAction::Status => DaemonCommand::Status,
                DaemonCtlAction::Shutdown => DaemonCommand::Shutdown,
            };

            let response = usbip_client::daemon::send_daemon_command(&socket_path, &cmd).await?;

            if response.status == "ok" {
                println!("OK: {}", response.message.as_deref().unwrap_or(""));
                if let Some(connections) = &response.connections {
                    if connections.is_empty() {
                        println!("  No active connections.");
                    } else {
                        println!("  Connections:");
                        for conn in connections {
                            let state_icon = match conn.state.as_str() {
                                "connected" => "\u{2713}",
                                "connecting" => "...",
                                _ => "!",
                            };
                            println!(
                                "  {} {} ({}) — {}",
                                state_icon, conn.busid, conn.server, conn.state
                            );
                        }
                    }
                }
            } else {
                eprintln!(
                    "Error: {}",
                    response.message.as_deref().unwrap_or("unknown error")
                );
                std::process::exit(1);
            }
        },
    }

    Ok(())
}
