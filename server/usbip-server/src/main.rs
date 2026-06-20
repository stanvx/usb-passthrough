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

    /// Bind to the IP address of the named network interface (e.g. `en0`).
    /// If set, the interface is resolved to a single IPv4 address at
    /// startup and used for both the USB/IP wire port (3240) and the
    /// REST API port (3241). Falls back to `--bind` if the interface
    /// cannot be resolved (e.g. interface is down).
    #[arg(long, value_name = "IFACE")]
    pub bind_iface: Option<String>,

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

/// Resolve the bind address to a single IP string.
///
/// Precedence: if `iface` is set, look up that interface's primary
/// IPv4 address and use it. If the lookup fails, fall back to the
/// explicit `bind` value (so the server can still come up on a host
/// that lacks the requested interface — e.g. a server that was
/// previously running on `en0` and is now on `eth0`).
///
/// `lookup` is injected so the resolver can be tested without touching
/// the host's real network interfaces.
pub fn resolve_bind_address(
    bind: &str,
    iface: Option<&str>,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> UsbIpResult<String> {
    match iface {
        Some(name) => match lookup(name) {
            Some(ip) => Ok(ip),
            None => {
                tracing::warn!(
                    iface = %name,
                    "could not resolve --bind-iface; falling back to --bind"
                );
                Ok(bind.to_string())
            },
        },
        None => Ok(bind.to_string()),
    }
}

/// Look up the primary IPv4 address of a network interface by name.
///
/// Returns `None` if the interface has no IPv4 address, doesn't exist,
/// or the platform doesn't support the lookup API.
pub fn lookup_interface_ip(name: &str) -> Option<String> {
    interface_lookup::lookup(name)
}

#[cfg(unix)]
mod interface_lookup {
    /// Look up the primary IPv4 address of `name` via `getifaddrs`.
    pub fn lookup(name: &str) -> Option<String> {
        // SAFETY: `getifaddrs` returns a linked list of `ifaddrs` nodes
        // for every interface on the host. We free the head pointer with
        // `freeifaddrs` and only read `ifa_name` (C string) and `ifa_addr`
        // (sockaddr). The list is allocated by libc and remains valid
        // until `freeifaddrs` is called.
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        // SAFETY: `head` is a valid out-pointer for `getifaddrs`.
        if unsafe { libc::getifaddrs(&mut head) } != 0 {
            return None;
        }

        let mut result: Option<String> = None;
        let mut cursor = head;
        while !cursor.is_null() {
            // SAFETY: `cursor` is a valid `ifaddrs` node until we walk past it.
            let ifa = unsafe { &*cursor };
            // SAFETY: `ifa_name` is a NUL-terminated C string owned by libc.
            let ifa_name = unsafe { std::ffi::CStr::from_ptr(ifa.ifa_name) }.to_str().unwrap_or("");
            if ifa_name == name && !ifa.ifa_addr.is_null() {
                // SAFETY: `ifa_addr` is a `sockaddr` for the family the
                // node describes. We've already checked non-null.
                let sa_family = unsafe { (*ifa.ifa_addr).sa_family } as i32;
                if sa_family == libc::AF_INET {
                    // SAFETY: `AF_INET` guarantees the address is a
                    // `sockaddr_in`; cast and read the IPv4 byte.
                    let sin = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in) };
                    let octets = sin.sin_addr.s_addr.to_ne_bytes();
                    result =
                        Some(format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3]));
                    break;
                }
            }
            cursor = ifa.ifa_next;
        }

        // SAFETY: `head` was returned by `getifaddrs` and is the only
        // pointer we need to free — `ifa_next` walks the list in place.
        unsafe { libc::freeifaddrs(head) };
        result
    }
}

#[cfg(not(unix))]
mod interface_lookup {
    /// Stub for non-Unix platforms (e.g. Windows). The CLI flag is
    /// accepted but the lookup always returns `None`; the operator
    /// can still use `--bind` directly.
    pub fn lookup(_name: &str) -> Option<String> {
        None
    }
}

#[tokio::main]
async fn main() -> UsbIpResult<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cli = Cli::parse();

    // Resolve --bind-iface (if set) to a single IP. The same address
    // drives both the USB/IP wire port and the REST API port, so the
    // operator only names the interface once.
    let bind_address =
        resolve_bind_address(&cli.bind, cli.bind_iface.as_deref(), &lookup_interface_ip)?;

    let config = ServerConfig {
        bind_address,
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

    #[test]
    fn cli_bind_iface_flag_accepted() {
        // `--bind-iface en0` → bind_iface is Some("en0"); bind stays at default.
        let cli = Cli::parse_from(["usbip-server", "--bind-iface", "en0"]);
        assert_eq!(cli.bind_iface.as_deref(), Some("en0"));
    }

    #[test]
    fn resolve_bind_address_uses_iface_ip() {
        // When --bind-iface is set, resolve_bind_address must use the
        // resolver to translate the interface name into an IP, not the
        // --bind default. The resolver is injected so the test does not
        // depend on the host's real network interfaces.
        let lookup: Box<dyn Fn(&str) -> Option<String> + Send + Sync> =
            Box::new(|name| match name {
                "en0" => Some("192.168.1.21".to_string()),
                _ => None,
            });
        let got = resolve_bind_address("0.0.0.0", Some("en0"), &lookup).unwrap();
        assert_eq!(got, "192.168.1.21");
    }

    #[test]
    fn resolve_bind_address_falls_back_when_iface_missing() {
        // If the interface cannot be resolved, fall back to the --bind
        // value rather than crashing — the user may have brought the
        // interface up later, or be running on a host with no resolvable
        // interfaces.
        let lookup: Box<dyn Fn(&str) -> Option<String> + Send + Sync> = Box::new(|_| None);
        let got = resolve_bind_address("10.0.0.5", Some("eth9"), &lookup).unwrap();
        assert_eq!(got, "10.0.0.5");
    }

    #[test]
    fn resolve_bind_address_passthrough_when_no_iface() {
        // No --bind-iface: the resolver is never consulted; --bind is
        // returned unchanged.
        let lookup: Box<dyn Fn(&str) -> Option<String> + Send + Sync> =
            Box::new(|_| panic!("resolver must not be called without --bind-iface"));
        let got = resolve_bind_address("127.0.0.1", None, &lookup).unwrap();
        assert_eq!(got, "127.0.0.1");
    }

    #[tokio::test]
    async fn bind_address_from_iface_resolution_binds_only_to_that_ip() {
        // The audit's acceptance criterion: when --bind-iface resolves
        // to a specific IP, the listener must bind only to that IP,
        // not 0.0.0.0. We exercise the resolution + bind path on
        // 127.0.0.1 (loopback, always present) and verify the
        // resulting socket reports that address — not 0.0.0.0.
        let lookup: Box<dyn Fn(&str) -> Option<String> + Send + Sync> =
            Box::new(|name| if name == "lo0" { Some("127.0.0.1".into()) } else { None });
        let resolved = resolve_bind_address("0.0.0.0", Some("lo0"), &lookup).unwrap();
        // Bind on an ephemeral port at the resolved address.
        let listener = tokio::net::TcpListener::bind(format!("{resolved}:0")).await.unwrap();
        let local = listener.local_addr().unwrap();
        assert_eq!(local.ip().to_string(), "127.0.0.1");
        assert_ne!(local.ip().to_string(), "0.0.0.0");
        drop(listener);
    }
}
