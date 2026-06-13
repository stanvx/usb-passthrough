//! USB backend trait, `UrbTransferResult`, and default `LibusbBackend`.
//!
//! Provides the `UsbBackend` trait for platform-agnostic USB device
//! management and the default libusb-based implementation.

use std::collections::HashMap;
use std::sync::Mutex;
use tracing::warn;

use rusb::{Context, Device, DeviceHandle, UsbContext};

use usbip_core::error::*;
use usbip_core::protocol::{UsbIpDeviceEntry, U16BE, U32BE};
use usbip_core::urb::UsbIpCmdSubmit;

/// Result of a single URB transfer.
#[derive(Debug, Clone)]
pub struct UrbTransferResult {
    pub status: i32,
    pub actual_length: u32,
    pub data: Vec<u8>,
}

/// Platform-agnostic USB backend trait.
pub trait UsbBackend: Send + Sync {
    fn list_devices(&self) -> Vec<UsbIpDeviceEntry>;
    fn get_device_entry(&self, busid: &str) -> Option<UsbIpDeviceEntry>;
    fn claim_device(&self, busid: &str) -> UsbIpResult<()>;
    fn get_descriptor_tree(&self, busid: &str) -> UsbIpResult<Vec<u8>>;
    fn execute_urb(
        &self,
        busid: &str,
        cmd: &UsbIpCmdSubmit,
        out_data: &[u8],
    ) -> UsbIpResult<UrbTransferResult>;
    fn release_device(&self, busid: &str) -> UsbIpResult<()>;
}

/// Default backend using libusb (rusb).
pub struct LibusbBackend {
    context: Context,
    /// busid -> (DeviceHandle, claimed)
    handles: Mutex<HashMap<String, (DeviceHandle<Context>, bool)>>,
}

impl LibusbBackend {
    pub fn new() -> UsbIpResult<Self> {
        let context = Context::new()?;
        Ok(Self { context, handles: Mutex::new(HashMap::new()) })
    }

    fn find_device(&self, busnum: u8, devnum: u8) -> UsbIpResult<Device<Context>> {
        let devices = self.context.devices()?;
        for device in devices.iter() {
            if device.bus_number() == busnum && device.address() == devnum {
                return Ok(device);
            }
        }
        Err(UsbIpError::from(ErrorKind::DeviceNotFound(format!("bus {} dev {}", busnum, devnum))))
    }
}

impl UsbBackend for LibusbBackend {
    fn list_devices(&self) -> Vec<UsbIpDeviceEntry> {
        let mut devices = Vec::new();
        let dev_list = match self.context.devices() {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to enumerate USB devices: {}", e);
                return devices;
            },
        };
        for device in dev_list.iter() {
            let desc = match device.device_descriptor() {
                Ok(d) => d,
                Err(_) => continue,
            };
            let busnum = device.bus_number();
            let devnum = device.address();
            let speed = device.speed() as u32;
            let busid = format!("{}-{}", busnum, devnum);
            let path = format!("/sys/bus/usb/devices/{}-{}", busnum, devnum);
            let mut entry = UsbIpDeviceEntry {
                path: [0u8; 256],
                busid: [0u8; 32],
                busnum: U32BE::new(busnum.into()),
                devnum: U32BE::new(devnum.into()),
                speed: U32BE::new(speed),
                id_vendor: U16BE::new(desc.vendor_id()),
                id_product: U16BE::new(desc.product_id()),
                bcd_device: U16BE::new(u16::from(desc.device_version().0)),
                b_device_class: desc.class_code(),
                b_device_sub_class: desc.sub_class_code(),
                b_device_protocol: desc.protocol_code(),
                b_configuration_value: 0,
                b_num_configurations: desc.num_configurations(),
                b_num_interfaces: 0,
            };
            let path_bytes = path.as_bytes();
            let copy_len = path_bytes.len().min(255);
            entry.path[..copy_len].copy_from_slice(&path_bytes[..copy_len]);
            let busid_bytes = busid.as_bytes();
            let copy_len = busid_bytes.len().min(31);
            entry.busid[..copy_len].copy_from_slice(&busid_bytes[..copy_len]);
            if let Ok(config) = device.config_descriptor(0) {
                entry.b_num_interfaces = config.num_interfaces();
                entry.b_configuration_value = config.number();
            }
            devices.push(entry);
        }
        devices
    }

    fn get_device_entry(&self, busid: &str) -> Option<UsbIpDeviceEntry> {
        let (busnum, devnum) = parse_busid(busid).ok()?;
        self.list_devices()
            .into_iter()
            .find(|d| d.busnum.get() == busnum as u32 && d.devnum.get() == devnum as u32)
    }

    fn claim_device(&self, busid: &str) -> UsbIpResult<()> {
        let (busnum, devnum) = parse_busid(busid)?;
        let device = self.find_device(busnum, devnum)?;
        let handle = device.open()?;
        let config = device.config_descriptor(0)?;
        for iface_idx in 0..config.num_interfaces() {
            let iface_num = config
                .interfaces()
                .nth(iface_idx as usize)
                .and_then(|i| i.descriptors().next())
                .map(|d| d.interface_number());
            if let Some(num) = iface_num {
                let _ = handle.detach_kernel_driver(num);
                handle.claim_interface(num)?;
            }
        }
        self.handles.lock().unwrap().insert(busid.to_string(), (handle, true));
        Ok(())
    }

    fn get_descriptor_tree(&self, busid: &str) -> UsbIpResult<Vec<u8>> {
        let (busnum, devnum) = parse_busid(busid)?;
        let device = self.find_device(busnum, devnum)?;
        let desc = device.device_descriptor()?;
        let mut tree = Vec::new();
        tree.extend_from_slice(&desc_to_bytes(&desc));
        for config_idx in 0..desc.num_configurations() {
            let config = device.config_descriptor(config_idx)?;
            let bm_attributes = if config.self_powered() { 0x40 } else { 0 }
                | if config.remote_wakeup() { 0x20 } else { 0 };
            let desc_bytes = [
                config.length(),
                config.descriptor_type(),
                (config.total_length() & 0xFF) as u8,
                ((config.total_length() >> 8) & 0xFF) as u8,
                config.num_interfaces(),
                config.number(),
                config.description_string_index().unwrap_or(0),
                bm_attributes,
                config.max_power() as u8,
            ];
            tree.extend_from_slice(&desc_bytes);
            for iface in config.interfaces() {
                for iface_desc in iface.descriptors() {
                    let iface_bytes = [
                        iface_desc.length(),
                        iface_desc.descriptor_type(),
                        iface_desc.interface_number(),
                        iface_desc.setting_number(),
                        iface_desc.num_endpoints(),
                        iface_desc.class_code(),
                        iface_desc.sub_class_code(),
                        iface_desc.protocol_code(),
                        iface_desc.description_string_index().unwrap_or(0),
                    ];
                    tree.extend_from_slice(&iface_bytes);
                    if iface_desc.class_code() == 0x03 {
                        let extra = iface_desc.extra();
                        if !extra.is_empty() {
                            tree.extend_from_slice(extra);
                        }
                    }
                    for ep_desc in iface_desc.endpoint_descriptors() {
                        let ep_bytes = [
                            ep_desc.length(),
                            ep_desc.descriptor_type(),
                            ep_desc.address(),
                            ep_desc.transfer_type() as u8,
                            (ep_desc.max_packet_size() & 0xFF) as u8,
                            ((ep_desc.max_packet_size() >> 8) & 0xFF) as u8,
                            ep_desc.interval(),
                        ];
                        tree.extend_from_slice(&ep_bytes);
                    }
                }
            }
        }
        Ok(tree)
    }

    fn execute_urb(
        &self,
        busid: &str,
        cmd: &UsbIpCmdSubmit,
        out_data: &[u8],
    ) -> UsbIpResult<UrbTransferResult> {
        let handles = self.handles.lock().unwrap();
        let (handle, _claimed) =
            handles.get(busid).ok_or_else(|| ErrorKind::DeviceNotFound(busid.into()))?;
        let ep_addr = cmd.ep_num() as u8;
        let timeout = std::time::Duration::from_millis(5000);
        let is_in = cmd.is_in();
        let is_control = cmd.is_control();
        if is_control {
            let setup_packet = cmd.setup;
            let bm_request_type = setup_packet[0];
            let b_request = setup_packet[1];
            let w_value = u16::from_le_bytes([setup_packet[2], setup_packet[3]]);
            let w_index = u16::from_le_bytes([setup_packet[4], setup_packet[5]]);
            let w_length = u16::from_le_bytes([setup_packet[6], setup_packet[7]]);
            if is_in {
                let mut buf = vec![0u8; w_length as usize];
                let len = handle.read_control(
                    bm_request_type,
                    b_request,
                    w_value,
                    w_index,
                    &mut buf,
                    timeout,
                )?;
                buf.truncate(len);
                Ok(UrbTransferResult { status: 0, actual_length: len as u32, data: buf })
            } else {
                let len = handle.write_control(
                    bm_request_type,
                    b_request,
                    w_value,
                    w_index,
                    out_data,
                    timeout,
                )?;
                Ok(UrbTransferResult { status: 0, actual_length: len as u32, data: Vec::new() })
            }
        } else if is_in {
            let max_size = cmd.data_len().max(512) as usize;
            let mut buf = vec![0u8; max_size];
            let len = handle.read_bulk(ep_addr, &mut buf, timeout)?;
            buf.truncate(len);
            Ok(UrbTransferResult { status: 0, actual_length: len as u32, data: buf })
        } else {
            let len = handle.write_bulk(ep_addr, out_data, timeout)?;
            Ok(UrbTransferResult { status: 0, actual_length: len as u32, data: Vec::new() })
        }
    }

    fn release_device(&self, busid: &str) -> UsbIpResult<()> {
        let mut handles = self.handles.lock().unwrap();
        if let Some((handle, _)) = handles.remove(busid) {
            let device = handle.device();
            if let Ok(config) = device.active_config_descriptor() {
                for iface in config.interfaces() {
                    for desc in iface.descriptors() {
                        let num = desc.interface_number();
                        let _ = handle.release_interface(num);
                        let _ = handle.attach_kernel_driver(num);
                    }
                }
            }
        }
        Ok(())
    }
}

fn desc_to_bytes(desc: &rusb::DeviceDescriptor) -> Vec<u8> {
    let usb_ver = desc.usb_version();
    let bcd_usb: u16 = (usb_ver.0 as u16) << 8 | usb_ver.1 as u16;
    vec![
        desc.length(),
        desc.descriptor_type(),
        (bcd_usb & 0xFF) as u8,
        ((bcd_usb >> 8) & 0xFF) as u8,
        desc.class_code(),
        desc.sub_class_code(),
        desc.protocol_code(),
        desc.max_packet_size(),
        (desc.vendor_id() & 0xFF) as u8,
        ((desc.vendor_id() >> 8) & 0xFF) as u8,
        (desc.product_id() & 0xFF) as u8,
        ((desc.product_id() >> 8) & 0xFF) as u8,
        (u16::from(desc.device_version().0) & 0xFF) as u8,
        ((u16::from(desc.device_version().0) >> 8) & 0xFF) as u8,
        desc.manufacturer_string_index().unwrap_or(0),
        desc.product_string_index().unwrap_or(0),
        desc.serial_number_string_index().unwrap_or(0),
        desc.num_configurations(),
    ]
}

fn parse_busid(busid: &str) -> UsbIpResult<(u8, u8)> {
    let parts: Vec<&str> = busid.split('-').collect();
    if parts.len() < 2 {
        return Err(UsbIpError::from(ErrorKind::DeviceNotFound(busid.into())));
    }
    let busnum: u8 = parts[0].parse().map_err(|_| ErrorKind::DeviceNotFound(busid.into()))?;
    let devnum: u8 = parts[1].parse().map_err(|_| ErrorKind::DeviceNotFound(busid.into()))?;
    Ok((busnum, devnum))
}

// ─── Test Helper: FakeBackend ──────────────────────────────────────────────

/// A fake USB backend that returns a fixed set of devices.
///
/// Used in tests for `UsbDeviceManager` and anywhere else that needs
/// a predictable backend.
#[cfg(test)]
pub(crate) struct FakeBackend {
    devices: Vec<UsbIpDeviceEntry>,
}

#[cfg(test)]
impl FakeBackend {
    pub fn new(devices: Vec<UsbIpDeviceEntry>) -> Self {
        Self { devices }
    }
}

#[cfg(test)]
impl UsbBackend for FakeBackend {
    fn list_devices(&self) -> Vec<UsbIpDeviceEntry> {
        self.devices.clone()
    }

    fn get_device_entry(&self, busid: &str) -> Option<UsbIpDeviceEntry> {
        self.devices.iter().find(|d| d.busid_str() == busid).cloned()
    }

    fn claim_device(&self, busid: &str) -> UsbIpResult<()> {
        if self.devices.iter().any(|d| d.busid_str() == busid) {
            Ok(())
        } else {
            Err(UsbIpError::from(ErrorKind::DeviceNotFound(busid.into())))
        }
    }

    fn release_device(&self, _busid: &str) -> UsbIpResult<()> {
        Ok(())
    }

    fn get_descriptor_tree(&self, busid: &str) -> UsbIpResult<Vec<u8>> {
        if self.devices.iter().any(|d| d.busid_str() == busid) {
            Ok(vec![0x12, 0x01, 0x00, 0x02])
        } else {
            Err(UsbIpError::from(ErrorKind::DeviceNotFound(busid.into())))
        }
    }

    fn execute_urb(
        &self,
        _busid: &str,
        _cmd: &UsbIpCmdSubmit,
        _out_data: &[u8],
    ) -> UsbIpResult<UrbTransferResult> {
        Ok(UrbTransferResult { status: 0, actual_length: 0, data: Vec::new() })
    }
}

/// Helper to create a test `UsbIpDeviceEntry`.
#[cfg(test)]
pub(crate) fn make_test_entry(busid: &str, vid: u16, pid: u16) -> UsbIpDeviceEntry {
    let mut entry = UsbIpDeviceEntry {
        path: [0u8; 256],
        busid: [0u8; 32],
        busnum: U32BE::new(1),
        devnum: U32BE::new(1),
        speed: U32BE::new(3),
        id_vendor: U16BE::new(vid),
        id_product: U16BE::new(pid),
        bcd_device: U16BE::new(0x0100),
        b_device_class: 0,
        b_device_sub_class: 0,
        b_device_protocol: 0,
        b_configuration_value: 1,
        b_num_configurations: 1,
        b_num_interfaces: 1,
    };
    let busid_bytes = busid.as_bytes();
    let copy_len = busid_bytes.len().min(31);
    entry.busid[..copy_len].copy_from_slice(&busid_bytes[..copy_len]);
    let path_str = format!("/sys/bus/usb/devices/{}", busid);
    let path_bytes = path_str.as_bytes();
    let copy_len = path_bytes.len().min(255);
    entry.path[..copy_len].copy_from_slice(&path_bytes[..copy_len]);
    entry
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── UsbBackend trait shape tests ────────────────────────────────

    #[test]
    fn test_backend_empty_list() {
        let backend = FakeBackend::new(vec![]);
        assert!(backend.list_devices().is_empty());
    }

    #[test]
    fn test_backend_returns_list() {
        let devices =
            vec![make_test_entry("1-1", 0x046d, 0xc261), make_test_entry("1-2", 0x8087, 0x0024)];
        let backend = FakeBackend::new(devices.clone());
        let listed = backend.list_devices();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].vid(), 0x046d);
        assert_eq!(listed[0].pid(), 0xc261);
    }

    #[test]
    fn test_backend_claim_existing_device() {
        let devices = vec![make_test_entry("1-1", 0x046d, 0xc261)];
        let backend = FakeBackend::new(devices);
        assert!(backend.claim_device("1-1").is_ok());
    }

    #[test]
    fn test_backend_claim_nonexistent_device() {
        let backend = FakeBackend::new(vec![]);
        let result = backend.claim_device("999-999");
        assert!(result.is_err());
    }

    #[test]
    fn test_backend_release_device() {
        let backend = FakeBackend::new(vec![]);
        assert!(backend.release_device("1-1").is_ok());
    }

    #[test]
    fn test_backend_descriptor_tree_ok() {
        let devices = vec![make_test_entry("1-1", 0x046d, 0xc261)];
        let backend = FakeBackend::new(devices);
        let tree = backend.get_descriptor_tree("1-1").unwrap();
        assert_eq!(tree[0], 0x12);
        assert_eq!(tree[1], 0x01);
    }

    #[test]
    fn test_backend_descriptor_tree_error() {
        let backend = FakeBackend::new(vec![]);
        let result = backend.get_descriptor_tree("999-999");
        assert!(result.is_err());
    }

    #[test]
    fn test_backend_execute_urb_ok() {
        let devices = vec![make_test_entry("1-1", 0x046d, 0xc261)];
        let backend = FakeBackend::new(devices);
        let cmd = UsbIpCmdSubmit {
            seqnum: U32BE::new(1),
            devid: U32BE::new(1),
            direction: U32BE::new(0),
            ep: U32BE::new(0x02),
            transfer_flags: U32BE::new(0),
            transfer_buffer_length: U32BE::new(0),
            start_frame: U32BE::new(0),
            number_of_packets: U32BE::new(0),
            interval: U32BE::new(0),
            setup: [0u8; 8],
        };
        let result = backend.execute_urb("1-1", &cmd, &[]).unwrap();
        assert_eq!(result.status, 0);
        assert!(result.data.is_empty());
    }

    #[test]
    fn test_backend_trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FakeBackend>();
    }

    #[test]
    fn test_get_device_entry_found() {
        let devices = vec![make_test_entry("1-1", 0x046d, 0xc261)];
        let backend = FakeBackend::new(devices);
        let entry = backend.get_device_entry("1-1");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().vid(), 0x046d);
    }

    #[test]
    fn test_get_device_entry_not_found() {
        let backend = FakeBackend::new(vec![]);
        assert!(backend.get_device_entry("999-999").is_none());
    }
}
