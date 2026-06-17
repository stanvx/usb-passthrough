//! USB/IP Server — CLI entry point.

use clap::{Parser, Subcommand};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use usbip_core::error::UsbIpResult;
use usbip_server::{metrics, BandwidthLimit, Server, ServerConfig};

#[derive(Parser, Debug, Clone, PartialEq)]
#[command(name = "usbip-server")]
#[command(about = "USB/IP Server — Export USB devices over the network")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Bind address (default: 0.0.0.0)
    #[arg(short, long, default_value = "0.0.0.0")]
    pub bind: String,

    /// TCP port (default: 3240)
    #[arg(short, long, default_value_t = 3240)]
    pub port: u16,

    /// REST API + WebSocket port. When set, the server runs the USB/IP
    /// wire listener and the axum API in parallel. Omit for wire-only.
    #[arg(long, default_missing_value = "3241", num_args = 0..=1)]
    pub api_port: Option<u16>,

    /// Only allow these VID:PID pairs (can be specified multiple times)
    #[arg(long, value_parser = parse_vid_pid)]
    pub allow: Vec<(u16, u16)>,

    /// Disable connection confirmation prompt
    #[arg(long)]
    pub no_confirm: bool,

    /// Enable AES-256-GCM encryption
    #[arg(long)]
    pub encrypt: bool,

    /// Prometheus metrics port (if set, serves /metrics on this port)
    #[arg(long)]
    pub metrics_port: Option<u16>,
}

impl Default for Cli {
    fn default() -> Self {
        Self::parse_from(["usbip-server"])
    }
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum Commands {
    /// List exportable USB devices
    List,
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

#[tokio::main]
async fn main() -> UsbIpResult<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cli = Cli::parse();

    let config = ServerConfig {
        bind_address: cli.bind,
        port: cli.port,
        allowed_vid_pid: cli.allow,
        require_confirmation: !cli.no_confirm,
        encryption_enabled: cli.encrypt,
        tcp_nodelay: true,
        max_bandwidth: BandwidthLimit::unlimited(),
        per_client_bandwidth: None,
    };

    let server = Server::new(config.clone()).await?;

    // Set encryption metric
    if cli.encrypt {
        metrics::ENCRYPTION_ENABLED.set(1);
    }

    // Start the Prometheus metrics server if a port was specified
    if let Some(metrics_port) = cli.metrics_port {
        let metrics_addr = format!("{}:{}", config.bind_address, metrics_port);
        let metrics_router = metrics::build_metrics_router();
        let metrics_listener = tokio::net::TcpListener::bind(&metrics_addr).await?;
        tracing::info!("Prometheus metrics listening on {}", metrics_addr);
        tokio::spawn(async move {
            axum::serve(metrics_listener, metrics_router).await.unwrap_or_else(|e| {
                tracing::error!("Metrics server error: {}", e);
            });
        });
    }

    match cli.command {
        Some(Commands::List) => {
            let devices = server.exportable_devices().await;
            println!("Exportable USB devices:");
            for dev in devices {
                println!(
                    "  {:04x}:{:04x}  {}  {}  speed={}",
                    dev.vid(),
                    dev.pid(),
                    dev.busid_str(),
                    dev.path_str(),
                    dev.speed_val(),
                );
            }
        },
        None => {
            if let Some(api_port) = cli.api_port {
                server.run_with_api(api_port).await?;
            } else {
                server.run().await?;
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_default_has_no_api_port() {
        // Omitting --api-port: wire-only behaviour (matches today's default).
        let cli = Cli::parse_from(["usbip-server"]);
        assert_eq!(cli.api_port, None, "no --api-port should mean None");
        assert_eq!(cli.port, 3240);
        assert_eq!(cli.bind, "0.0.0.0");
    }

    #[test]
    fn cli_api_port_with_value() {
        // `--api-port 5000` → Some(5000).
        let cli = Cli::parse_from(["usbip-server", "--api-port", "5000"]);
        assert_eq!(cli.api_port, Some(5000));
    }

    #[test]
    fn cli_api_port_without_value_defaults_to_3241() {
        // `--api-port` (no value) → Some(3241), per acceptance criteria.
        let cli = Cli::parse_from(["usbip-server", "--api-port"]);
        assert_eq!(cli.api_port, Some(3241));
    }
}
