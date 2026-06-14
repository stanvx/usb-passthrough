//! mDNS service discovery for USB/IP.
//!
//! Browses `_usbip._tcp.local` to find servers on the local network.

use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::net::SocketAddr;
use std::time::Duration;
use tracing::info;

use usbip_core::error::*;

pub struct MdnsBrowser {
    daemon: ServiceDaemon,
}

impl MdnsBrowser {
    pub fn new() -> UsbIpResult<Self> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| ErrorKind::NotSupported(format!("mDNS init failed: {}", e)))?;

        Ok(Self { daemon })
    }

    /// Browse for USB/IP servers. Returns list of SocketAddr after a short scan.
    pub fn browse(&self) -> UsbIpResult<Vec<SocketAddr>> {
        let service_type = "_usbip._tcp.local.";
        let receiver = self
            .daemon
            .browse(service_type)
            .map_err(|e| ErrorKind::NotSupported(format!("mDNS browse failed: {}", e)))?;

        let mut servers = Vec::new();

        // Wait for responses (2 second scan)
        let timeout = Duration::from_secs(2);
        let start = std::time::Instant::now();

        loop {
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                break;
            }

            match receiver.recv_timeout(remaining) {
                Ok(event) => {
                    if let ServiceEvent::ServiceResolved(info) = event {
                        let addr = info.get_addresses();
                        for a in addr.iter() {
                            let port = info.get_port();
                            let sock = SocketAddr::new(*a, port);
                            if !servers.contains(&sock) {
                                info!("mDNS discovered USB/IP server at {}", sock);
                                servers.push(sock);
                            }
                        }
                    }
                },
                Err(_) => break, // timeout
            }
        }

        Ok(servers)
    }
}
