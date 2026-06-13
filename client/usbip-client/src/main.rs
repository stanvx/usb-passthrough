//! USB/IP Client — CLI entry point.

use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use usbip_client::{Client, ClientConfig};
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
    /// Import a USB device from a server
    Connect {
        /// Server address (host:port)
        server: SocketAddr,
        /// Device bus ID to import
        busid: String,
        /// Don't auto-reconnect on disconnect
        #[arg(long)]
        no_reconnect: bool,
    },
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
    }

    Ok(())
}
