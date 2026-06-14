//! Linux/Android VHCI backend.
//!
//! Uses the vhci-hcd kernel module which creates virtual USB host
//! controllers via sysfs (`/sys/devices/platform/vhci_hcd.0`).
//! Android shares the same sysfs interface.

use std::fs;
use std::path::PathBuf;

use tracing::{debug, info, warn};

use usbip_core::error::*;

use crate::vhci::{VhciBackend, VhciDevice};

/// Linux/Android VHCI backend using sysfs.
pub(super) struct LinuxVhciBackend {
    /// Sysfs path to the VHCI controller.
    sysfs_path: PathBuf,
    /// Number of VHCI ports to manage.
    #[allow(dead_code)]
    num_ports: u32,
}

impl LinuxVhciBackend {
    pub(super) fn new() -> UsbIpResult<Self> {
        let vhci_path = PathBuf::from("/sys/devices/platform/vhci_hcd.0");
        let num_ports = if vhci_path.exists() {
            fs::read_dir(vhci_path.join("status")).map(|d| d.count() as u32).unwrap_or(8)
        } else {
            warn!("vhci-hcd kernel module not loaded. Trying to load...");
            if std::process::Command::new("modprobe").arg("vhci-hcd").status().is_err() {
                return Err(ErrorKind::NotSupported(
                    "vhci-hcd kernel module not available. Install with: \
                     sudo modprobe vhci-hcd"
                        .into(),
                )
                .into());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
            8
        };

        Ok(Self { sysfs_path: vhci_path, num_ports })
    }
}

impl VhciBackend for LinuxVhciBackend {
    fn create_device(
        &self,
        entry: &usbip_core::protocol::UsbIpDeviceEntry,
        _descriptors: &[u8],
    ) -> UsbIpResult<VhciDevice> {
        let port = self.find_free_port()?;
        let attach_path = self.sysfs_path.join("attach");
        let devid = port;
        let speed = entry.speed_val();
        let attach_str = format!("{} {} {}\n", port, devid, speed);
        fs::write(&attach_path, attach_str)?;

        info!("VHCI: attached device {} at port {} (speed={})", entry.busid_str(), port, speed,);

        Ok(VhciDevice {
            port,
            devid,
            busid: entry.busid_str().to_string(),
            vid: entry.vid(),
            pid: entry.pid(),
        })
    }

    fn complete_urb(
        &self,
        seqnum: u32,
        devid: u32,
        status: i32,
        actual_length: u32,
        data: &[u8],
    ) -> UsbIpResult<()> {
        let port = devid;
        let complete_path = self.sysfs_path.join(format!("port{}/urb_complete", port));

        let mut buf = Vec::new();
        buf.extend_from_slice(&seqnum.to_be_bytes());
        buf.extend_from_slice(&(status as u32).to_be_bytes());
        buf.extend_from_slice(&actual_length.to_be_bytes());
        buf.extend_from_slice(data);

        fs::write(&complete_path, &buf)?;
        Ok(())
    }

    fn cancel_urb(&self, seqnum: u32, devid: u32) -> UsbIpResult<()> {
        debug!("VHCI: cancel URB seq={} dev={}", seqnum, devid);
        let port = devid;
        let unlink_path = self.sysfs_path.join(format!("port{}/urb_unlink", port));
        let buf = seqnum.to_be_bytes();
        let _ = fs::write(&unlink_path, buf);
        Ok(())
    }

    fn remove_device(&self, port: u32) -> UsbIpResult<()> {
        let detach_path = self.sysfs_path.join("detach");
        let detach_str = format!("{}\n", port);
        fs::write(&detach_path, detach_str)?;
        info!("VHCI: detached device at port {}", port);
        Ok(())
    }

    fn find_free_port(&self) -> UsbIpResult<u32> {
        let status_path = self.sysfs_path.join("status");
        let status = fs::read_to_string(&status_path).unwrap_or_default();
        for (port, line) in status.lines().enumerate() {
            if line.contains("Port") && !line.contains("Attached") {
                return Ok(port as u32);
            }
        }
        warn!("No free VHCI ports found, attempting port 0");
        Ok(0)
    }
}
