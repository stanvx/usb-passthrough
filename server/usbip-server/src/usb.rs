//! USB device management via the platform-agnostic `UsbBackend` trait.
//!
//! `UsbDeviceManager` delegates all operations to a pluggable backend,
//! defaulting to `LibusbBackend` on all platforms.  On macOS the
//! `IokitBackend` is also available.
//!
//! The `LibusbBackend` module provides the standard libusb-based
//! implementation for enumeration, claiming, and URB submission.

use std::collections::HashMap;
use std::sync::Mutex;

use tracing::debug;

use usbip_core::error::{ErrorKind, UsbIpError, UsbIpResult};
use usbip_core::protocol::UsbIpDeviceEntry;
use usbip_core::urb::UsbIpCmdSubmit;

use crate::api::DeviceLister;
use crate::usb_backend::{LibusbBackend, UrbTransferResult, UsbBackend};

/// Manages USB devices for the server, delegating to a backend.
///
/// Backend is chosen at build time or runtime:
/// - Default: `LibusbBackend` (rusb) — works on Linux, macOS, Windows
/// - macOS: `IokitBackend` (native I/O Kit) — for full macOS integration
///
/// The backend is pluggable via the `UsbBackend` trait.
pub struct UsbDeviceManager {
    /// The active USB backend.
    backend: Box<dyn UsbBackend>,
    /// busid → (backend status tracking, if needed)
    handles: Mutex<HashMap<String, bool>>,
}

impl UsbDeviceManager {
    /// Create with the default backend (libusb/rusb).
    pub fn new() -> UsbIpResult<Self> {
        let backend = Box::new(LibusbBackend::new()?);
        Ok(Self { backend, handles: Mutex::new(HashMap::new()) })
    }

    /// Create with a specific backend (e.g., `IokitBackend`).
    pub fn with_backend(backend: Box<dyn UsbBackend>) -> Self {
        Self { backend, handles: Mutex::new(HashMap::new()) }
    }

    /// List all USB devices on the system.
    pub fn list_devices(&self) -> Vec<UsbIpDeviceEntry> {
        self.backend.list_devices()
    }

    /// List devices, filtered by an allowlist of (VID, PID) pairs.
    ///
    /// If `allowed` is empty, all devices are returned.
    /// Only devices whose VID:PID match an entry in `allowed` are included.
    pub fn list_exportable_devices(&self, allowed: &[(u16, u16)]) -> Vec<UsbIpDeviceEntry> {
        let all = self.list_devices();
        if allowed.is_empty() {
            return all;
        }
        all.into_iter()
            .filter(|d| allowed.iter().any(|(vid, pid)| d.vid() == *vid && d.pid() == *pid))
            .collect()
    }

    /// Get a device entry by busid.
    pub fn get_device_entry(&self, busid: &str) -> Option<UsbIpDeviceEntry> {
        let (busnum, devnum) = parse_busid(busid).ok()?;
        self.list_devices()
            .into_iter()
            .find(|d| d.busnum.get() == busnum as u32 && d.devnum.get() == devnum as u32)
    }

    /// Claim a device (detach kernel driver, claim interface).
    pub fn claim_device(&self, busid: &str) -> UsbIpResult<()> {
        self.backend.claim_device(busid)?;
        self.handles.lock().unwrap().insert(busid.to_string(), true);
        debug!("Claimed device: {}", busid);
        Ok(())
    }

    /// Get the full USB descriptor tree for a device.
    pub fn get_descriptor_tree(&self, busid: &str) -> UsbIpResult<Vec<u8>> {
        self.backend.get_descriptor_tree(busid)
    }

    /// Execute a URB (submit a USB transfer) on the physical device.
    pub fn execute_urb(
        &self,
        busid: &str,
        cmd: &UsbIpCmdSubmit,
        out_data: &[u8],
    ) -> UsbIpResult<UrbTransferResult> {
        let _handles = self.handles.lock().unwrap();
        if !_handles.contains_key(busid) {
            return Err(UsbIpError::from(ErrorKind::DeviceNotFound(busid.into())));
        }
        self.backend.execute_urb(busid, cmd, out_data)
    }

    /// Release a claimed device.
    pub fn release_device(&self, busid: &str) -> UsbIpResult<()> {
        let mut _handles = self.handles.lock().unwrap();
        _handles.remove(busid);
        self.backend.release_device(busid)?;
        debug!("Released device: {}", busid);
        Ok(())
    }
}

impl DeviceLister for UsbDeviceManager {
    fn list_devices(&self) -> Vec<UsbIpDeviceEntry> {
        self.list_devices()
    }
}

/// Parse a busid string ("busnum-devnum") into its numeric components.
fn parse_busid(busid: &str) -> UsbIpResult<(u8, u8)> {
    let parts: Vec<&str> = busid.split('-').collect();
    if parts.len() < 2 {
        return Err(UsbIpError::from(ErrorKind::DeviceNotFound(busid.into())));
    }
    let busnum: u8 = parts[0].parse().map_err(|_| ErrorKind::DeviceNotFound(busid.into()))?;
    let devnum: u8 = parts[1].parse().map_err(|_| ErrorKind::DeviceNotFound(busid.into()))?;
    Ok((busnum, devnum))
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usb_backend::make_test_entry;

    #[test]
    fn test_parse_busid_valid() {
        let (bus, dev) = parse_busid("3-2").unwrap();
        assert_eq!(bus, 3);
        assert_eq!(dev, 2);
    }

    #[test]
    fn test_parse_busid_invalid() {
        assert!(parse_busid("").is_err());
        assert!(parse_busid("abc").is_err());
        assert!(parse_busid("3").is_err());
    }

    #[test]
    fn test_parse_busid_multidigit() {
        let (bus, dev) = parse_busid("12-5").unwrap();
        assert_eq!(bus, 12);
        assert_eq!(dev, 5);
    }

    #[test]
    fn test_rusb_to_urb_status_mapping() {
        assert_eq!(usbip_core::error::rusb_to_urb_status(&rusb::Error::Io), -5);
        assert_eq!(usbip_core::error::rusb_to_urb_status(&rusb::Error::Timeout), -62);
        assert_eq!(usbip_core::error::rusb_to_urb_status(&rusb::Error::NoDevice), -19);
        assert_eq!(usbip_core::error::rusb_to_urb_status(&rusb::Error::NotSupported), -95);
    }

    /// The `with_backend` constructor accepts any `Box<dyn UsbBackend>`.
    #[test]
    fn test_with_backend_accepts_fake_backend() {
        use crate::usb_backend::FakeBackend;
        let fake = FakeBackend::new(vec![]);
        let mgr = UsbDeviceManager::with_backend(Box::new(fake));
        assert!(mgr.list_devices().is_empty());
    }

    /// Allowlisting with empty list returns all devices.
    #[test]
    fn test_list_exportable_devices_empty_allowlist() {
        use crate::usb_backend::FakeBackend;
        let entry = make_test_entry("1-1", 0x046d, 0xc261);
        let fake = FakeBackend::new(vec![entry]);
        let mgr = UsbDeviceManager::with_backend(Box::new(fake));

        let devs = mgr.list_exportable_devices(&[]);
        assert_eq!(devs.len(), 1);
        assert_eq!(devs[0].vid(), 0x046d);
    }

    /// Allowlisting filters out non-matching devices.
    #[test]
    fn test_list_exportable_devices_filters() {
        use crate::usb_backend::FakeBackend;
        let dev1 = make_test_entry("1-1", 0x046d, 0xc261);
        let dev2 = make_test_entry("1-2", 0x8087, 0x0024);
        let fake = FakeBackend::new(vec![dev1, dev2]);
        let mgr = UsbDeviceManager::with_backend(Box::new(fake));

        // Allow only a specific device (046d:c261)
        let devs = mgr.list_exportable_devices(&[(0x046d, 0xc261)]);
        assert_eq!(devs.len(), 1);
        assert_eq!(devs[0].vid(), 0x046d);
        assert_eq!(devs[0].pid(), 0xc261);
    }

    /// Allowlisting with multiple VID:PID pairs works.
    #[test]
    fn test_list_exportable_devices_multiple_allowed() {
        use crate::usb_backend::FakeBackend;
        let dev1 = make_test_entry("1-1", 0x046d, 0xc261);
        let dev2 = make_test_entry("1-2", 0x8087, 0x0024);
        let dev3 = make_test_entry("1-3", 0x1234, 0x5678);
        let fake = FakeBackend::new(vec![dev1, dev2, dev3]);
        let mgr = UsbDeviceManager::with_backend(Box::new(fake));

        let devs = mgr.list_exportable_devices(&[(0x046d, 0xc261), (0x8087, 0x0024)]);
        assert_eq!(devs.len(), 2);
    }
}
