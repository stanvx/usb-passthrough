/// mDNS service advertisement for USB/IP.
///
/// Publishes `_usbip._tcp.local` so clients can discover the server
/// without knowing its IP address.
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tracing::{error, info, warn};

use usbip_core::error::*;

use crate::api::{DiscoveredServer, MdnsBrowser};

pub struct MdnsAdvertiser {
    daemon: ServiceDaemon,
    service_name: String,
    port: u16,
}

impl MdnsAdvertiser {
    pub fn new(port: u16) -> UsbIpResult<Self> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| ErrorKind::NotSupported(format!("mDNS init failed: {}", e)))?;

        let hostname = gethostname::gethostname().to_string_lossy().to_string();
        let service_name = format!("{}._usbip._tcp.local.", hostname);

        Ok(Self { daemon, service_name, port })
    }

    pub fn start(&self) -> UsbIpResult<()> {
        let local_ip = get_local_ip().unwrap_or(IpAddr::from([127, 0, 0, 1]));

        let properties = [("version", "1.1.1"), ("platform", std::env::consts::OS)];

        let service_info = ServiceInfo::new(
            "_usbip._tcp.local.",
            "USB/IP Server",
            &self.service_name,
            local_ip,
            self.port,
            &properties[..],
        )
        .map_err(|e| ErrorKind::NotSupported(format!("mDNS service creation failed: {}", e)))?;

        self.daemon
            .register(service_info)
            .map_err(|e| ErrorKind::NotSupported(format!("mDNS register failed: {}", e)))?;

        info!("mDNS advertised: {} on {}:{}", self.service_name, local_ip, self.port);
        Ok(())
    }
}

impl Drop for MdnsAdvertiser {
    fn drop(&mut self) {
        if let Err(e) = self.daemon.unregister(&self.service_name) {
            error!("mDNS unregister error: {}", e);
        }
        let _ = self.daemon.shutdown();
    }
}

/// Get the first non-loopback IPv4 address.
fn get_local_ip() -> Option<IpAddr> {
    use std::net::UdpSocket;

    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:80").ok()?;
    match socket.local_addr().ok()? {
        std::net::SocketAddr::V4(addr) => Some(IpAddr::V4(*addr.ip())),
        std::net::SocketAddr::V6(addr) => Some(IpAddr::V6(*addr.ip())),
    }
}

/// mDNS browser for `_usbip._tcp.local`.
///
/// Used by the server's REST API (`POST /api/scan`) to discover remote
/// USB/IP servers on the LAN. Implements the `api::MdnsBrowser` trait
/// so tests can swap it out.
pub struct MdnsBrowserImpl {
    daemon: ServiceDaemon,
}

impl MdnsBrowserImpl {
    pub fn new() -> UsbIpResult<Self> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| ErrorKind::NotSupported(format!("mDNS init failed: {}", e)))?;
        Ok(Self { daemon })
    }
}

impl MdnsBrowser for MdnsBrowserImpl {
    fn browse(&self, timeout_secs: u32) -> Vec<DiscoveredServer> {
        let receiver = match self.daemon.browse("_usbip._tcp.local.") {
            Ok(r) => r,
            Err(e) => {
                warn!("mDNS browse failed: {}", e);
                return Vec::new();
            },
        };

        let timeout = Duration::from_secs(timeout_secs as u64);
        let deadline = std::time::Instant::now() + timeout;
        let mut by_name: HashMap<String, DiscoveredServer> = HashMap::new();

        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match receiver.recv_timeout(remaining) {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    let host = info
                        .get_addresses()
                        .iter()
                        .next()
                        .map(|a| a.to_string())
                        .unwrap_or_default();
                    let txt: HashMap<String, String> = info
                        .get_properties()
                        .iter()
                        .map(|p| (p.key().to_string(), p.val_str().to_string()))
                        .collect();
                    by_name.entry(info.get_fullname().to_string()).or_insert(DiscoveredServer {
                        host,
                        port: info.get_port(),
                        txt,
                    });
                },
                Ok(ServiceEvent::SearchStopped(_)) => break,
                Err(_) => break,
                _ => {},
            }
        }

        let _ = self.daemon.shutdown();
        by_name.into_values().collect()
    }
}
