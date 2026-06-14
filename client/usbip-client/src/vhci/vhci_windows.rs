//! Windows VHCI backend.
//!
//! Uses the usbip-win2 kernel driver (vhci.sys) which registers a
//! device interface communicated with via IOCTL (`\\.\USBIP-VHCI`).

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use tracing::info;
use winapi::um::fileapi::CreateFileW;
use winapi::um::handleapi::CloseHandle;
use winapi::um::ioapiset::DeviceIoControl;
use winapi::um::winbase::OPEN_EXISTING;
use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ, GENERIC_WRITE};

use usbip_core::error::*;
use usbip_core::protocol::UsbIpDeviceEntry;

use crate::vhci::{VhciBackend, VhciDevice};

const IOCTL_USBIP_VHCI_ATTACH: u32 = 0x220004;

/// Windows VHCI backend using usbip-win2 IOCTL interface.
pub(super) struct WindowsVhciBackend;

impl VhciBackend for WindowsVhciBackend {
    fn create_device(
        &self,
        entry: &UsbIpDeviceEntry,
        descriptors: &[u8],
    ) -> UsbIpResult<VhciDevice> {
        let port = self.find_free_port()?;
        let devid = port;

        // Build descriptor block for the driver
        let desc_block = build_windows_descriptor_block(entry, descriptors);

        // Open device handle
        let device_path: Vec<u16> =
            OsStr::new(r"\\.\USBIP-VHCI").encode_wide().chain(std::iter::once(0)).collect();

        let handle = unsafe {
            CreateFileW(
                device_path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };

        if handle == winapi::um::handleapi::INVALID_HANDLE_VALUE {
            return Err(ErrorKind::NotSupported(
                r"Cannot open \\.\USBIP-VHCI. Is usbip-win2 driver installed?".into(),
            ));
        }

        // Build IOCTL input buffer: port, devid, speed, descriptor_block
        let mut input = Vec::with_capacity(12 + desc_block.len());
        input.extend_from_slice(&port.to_le_bytes());
        input.extend_from_slice(&devid.to_le_bytes());
        input.extend_from_slice(&entry.speed_val().to_le_bytes());
        input.extend_from_slice(&desc_block);

        let mut bytes_returned: u32 = 0;
        let result = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_USBIP_VHCI_ATTACH,
                input.as_ptr() as *mut _,
                input.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut bytes_returned,
                std::ptr::null_mut(),
            )
        };

        unsafe {
            CloseHandle(handle);
        }

        if result == 0 {
            return Err(ErrorKind::NotSupported("VHCI attach IOCTL failed".into()));
        }

        info!("Windows VHCI: attached device at port {}", port);

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
        _devid: u32,
        _seqnum: u32,
        _status: i32,
        _actual_length: u32,
        _data: &[u8],
    ) -> UsbIpResult<()> {
        // IOCTL_USBIP_VHCI_COMPLETE_URB (stub)
        Ok(())
    }

    fn cancel_urb(&self, _seqnum: u32, _devid: u32) -> UsbIpResult<()> {
        Ok(())
    }

    fn remove_device(&self, _port: u32) -> UsbIpResult<()> {
        Ok(())
    }
}

/// Build a Windows-compatible descriptor block from USB descriptors.
fn build_windows_descriptor_block(_entry: &UsbIpDeviceEntry, descriptors: &[u8]) -> Vec<u8> {
    // Windows expects: total_length (4 bytes) + raw descriptor tree
    let mut block = Vec::with_capacity(4 + descriptors.len());
    block.extend_from_slice(&(descriptors.len() as u32).to_le_bytes());
    block.extend_from_slice(descriptors);
    block
}
