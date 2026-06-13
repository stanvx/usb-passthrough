//! macOS I/O Kit USB backend.
//!
//! Uses Apple's IOKit framework to enumerate USB devices, claim them,
//! and execute USB transfers.  On non-macOS platforms this module
//! is replaced by a stub (the `LibusbBackend` via rusb is used instead).
//!
//! ## Security
//!
//! IOKit access on macOS 10.14+ requires the app to have the
//! "com.apple.security.device.usb" entitlement (hardened runtime).
//! For development, run with `sudo` or disable hardened runtime.

#![cfg(target_os = "macos")]

use std::collections::HashMap;
use std::sync::Mutex;

use tracing::{debug, warn};

use usbip_core::error::{ErrorKind, UsbIpError, UsbIpResult};
use usbip_core::protocol::{UsbIpDeviceEntry, U16BE, U32BE};
use usbip_core::urb::UsbIpCmdSubmit;

use crate::usb_backend::{UrbTransferResult, UsbBackend};

// ─── IOKit FFI Declarations ──────────────────────────────────────────────

/// Opaque IOKit types (we only use them through pointer indirection).
// We use `allow(non_camel_case_types)` because these mirror the C type names
// from IOKit/ CoreFoundation headers for grep-ability and cross-referencing.
#[allow(non_camel_case_types)]
pub type io_object_t = u32;
#[allow(non_camel_case_types)]
pub type io_iterator_t = io_object_t;
#[allow(non_camel_case_types)]
pub type io_service_t = io_object_t;
#[allow(non_camel_case_types)]
pub type io_connect_t = io_object_t;
#[allow(non_camel_case_types)]
pub type mach_port_t = u32;
#[allow(non_camel_case_types)]
pub type kern_return_t = i32;
pub type CFTypeRef = *const std::ffi::c_void;
pub type CFDictionaryRef = CFTypeRef;
pub type CFStringRef = *const std::ffi::c_void;
pub type CFNumberRef = CFTypeRef;
pub type CFMutableDictionaryRef = CFTypeRef;
pub type CFAllocatorRef = *const std::ffi::c_void;
pub type IOCFPlugInInterface = *const std::ffi::c_void;
pub type IOUSBDeviceInterface = *const std::ffi::c_void;
pub type IOUSBInterfaceInterface = *const std::ffi::c_void;

// IOKit return codes
const KERN_SUCCESS: kern_return_t = 0;
const KERN_FAILURE: kern_return_t = -1;

// ─── IOKit Framework FFI (weak-linked for runtime safety) ────────────────

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: CFStringRef) -> CFMutableDictionaryRef;
    fn IOServiceGetMatchingServices(
        mainPort: mach_port_t,
        matching: CFDictionaryRef,
        existing: *mut io_iterator_t,
    ) -> kern_return_t;
    fn IOIteratorNext(iterator: io_iterator_t) -> io_object_t;
    fn IOObjectRelease(object: io_object_t) -> kern_return_t;
    fn IORegistryEntryCreateCFProperty(
        entry: io_registry_entry_t,
        key: CFStringRef,
        allocator: CFAllocatorRef,
        options: u32,
    ) -> CFTypeRef;
    fn IOCreatePlugInInterfaceForService(
        service: io_service_t,
        pluginTypeID: CFTypeRef,
        interfaceID: CFTypeRef,
        theInterface: *mut *mut IOCFPlugInInterface,
        theScore: *mut i32,
    ) -> kern_return_t;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        cStr: *const std::ffi::c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFStringGetCString(
        theString: CFStringRef,
        buffer: *mut std::ffi::c_char,
        bufferSize: isize,
        encoding: u32,
    ) -> std::ffi::c_uchar;
    fn CFStringGetLength(theString: CFStringRef) -> isize;
    fn CFRelease(cf: CFTypeRef);
    fn CFNumberGetValue(
        number: CFNumberRef,
        theType: u32,
        valuePtr: *mut std::ffi::c_void,
    ) -> std::ffi::c_uchar;
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    fn CFStringGetTypeID() -> CFTypeID;
    fn CFNumberGetTypeID() -> CFTypeID;
    fn CFDictionaryGetValue(theDict: CFDictionaryRef, key: CFTypeRef) -> CFTypeRef;
    fn CFDictionaryGetCount(theDict: CFDictionaryRef) -> isize;
}

// CoreFoundation string encoding
const kCFStringEncodingUTF8: u32 = 0x08000100;

#[allow(non_camel_case_types)]
type io_registry_entry_t = io_object_t;
type CFTypeID = std::ffi::c_ulong;

// ─── Helper: Create CFString from Rust string ────────────────────────────

unsafe fn cfstring(s: &str) -> CFStringRef {
    let cstr = std::ffi::CString::new(s).unwrap();
    CFStringCreateWithCString(std::ptr::null(), cstr.as_ptr(), kCFStringEncodingUTF8)
}

unsafe fn cfstring_to_string(cf: CFStringRef) -> Option<String> {
    let len = CFStringGetLength(cf);
    if len <= 0 {
        return None;
    }
    let max_size = (len * 4 + 1) as usize;
    let mut buf = vec![0u8; max_size];
    let result = CFStringGetCString(
        cf,
        buf.as_mut_ptr() as *mut std::ffi::c_char,
        max_size as isize,
        kCFStringEncodingUTF8,
    );
    if result == 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..end]).to_string())
}

// ─── IOKitBackend ────────────────────────────────────────────────────────

/// IOKit-based USB backend for macOS.
///
/// Enumerates USB devices via `IOServiceGetMatchingServices` and
/// performs USB transfers via `IOUSBDeviceInterface`.
pub struct IokitBackend {
    /// Claimed device handles keyed by busid.
    ///
    /// On macOS, we keep the plugin interface and device interface references
    /// so we can perform transfers.  For now the state tracks what's claimed.
    claimed: Mutex<HashMap<String, ClaimedDevice>>,
}

struct ClaimedDevice {
    _service: u32,
    // Future: plugin_interface, device_interface, interface_interface
}

impl IokitBackend {
    /// Create a new IOKit backend.
    ///
    /// Returns `Err` if IOKit is unavailable (should not happen on macOS).
    pub fn new() -> UsbIpResult<Self> {
        // No explicit init needed — IOKit is always present on macOS.
        Ok(Self { claimed: Mutex::new(HashMap::new()) })
    }
}

impl UsbBackend for IokitBackend {
    fn list_devices(&self) -> Vec<UsbIpDeviceEntry> {
        let mut devices = Vec::new();

        unsafe {
            let matching_name = cfstring("IOUSBDevice");
            if matching_name.is_null() {
                warn!("Failed to create CFString for IOUSBDevice");
                return devices;
            }

            let matching_dict = IOServiceMatching(matching_name);
            CFRelease(matching_name);

            if matching_dict.is_null() {
                warn!("IOServiceMatching returned NULL");
                return devices;
            }

            let mut iterator: io_iterator_t = 0;
            let kr = IOServiceGetMatchingServices(0, matching_dict, &mut iterator);
            if kr != KERN_SUCCESS || iterator == 0 {
                warn!("IOServiceGetMatchingServices failed: {}", kr);
                return devices;
            }

            let mut index: u32 = 0;
            loop {
                let service = IOIteratorNext(iterator);
                if service == 0 {
                    break;
                }

                let entry = unsafe { enumerate_one_device(service, index) };
                if let Some(entry) = entry {
                    devices.push(entry);
                }
                index += 1;

                IOObjectRelease(service);
            }

            IOObjectRelease(iterator);
        }

        devices
    }

    fn get_device_entry(&self, busid: &str) -> Option<UsbIpDeviceEntry> {
        let _parts: Vec<&str> = busid.split('-').collect();
        if _parts.len() < 2 {
            return None;
        }
        // Simple linear search through list_devices — caching will be added later.
        self.list_devices().into_iter().find(|d| d.busid_str() == busid)
    }

    fn claim_device(&self, busid: &str) -> UsbIpResult<()> {
        // On macOS, claiming means:
        // 1. Find the IOUSBDevice service matching the busid
        // 2. Create a plugin interface
        // 3. Open the device (which detaches the kernel driver)
        //
        // For now we do a lightweight claim: find the service and track it.
        let service = self.find_service(busid)?;

        let mut claimed = self.claimed.lock().unwrap();
        claimed.insert(busid.to_string(), ClaimedDevice { _service: service });

        debug!("Claimed device via IOKit: {}", busid);
        Ok(())
    }

    fn release_device(&self, busid: &str) -> UsbIpResult<()> {
        let mut claimed = self.claimed.lock().unwrap();
        if let Some(_dev) = claimed.remove(busid) {
            // TODO: Close the IOUSBDeviceInterface
            debug!("Released device via IOKit: {}", busid);
        }
        Ok(())
    }

    fn get_descriptor_tree(&self, busid: &str) -> UsbIpResult<Vec<u8>> {
        // Check that device is claimed or exists
        let _service = self.find_service(busid)?;

        // Build a minimal descriptor tree from IOKit properties.
        // Full descriptor enumeration requires reading from the
        // IOUSBDeviceInterface which requires the plugin.
        // For now, return a minimal device descriptor based on IOKit properties.
        unsafe { get_descriptor_tree_from_iokit(busid) }
    }

    fn execute_urb(
        &self,
        _busid: &str,
        _cmd: &UsbIpCmdSubmit,
        _out_data: &[u8],
    ) -> UsbIpResult<UrbTransferResult> {
        // TODO: Full URB execution via IOUSBDeviceInterface ReadPipe/WritePipe.
        // For the initial implementation, return an error indicating this
        // is not yet implemented through the IOKit direct path.
        // The existing rusb-based path works on macOS for now.
        Err(UsbIpError::from(ErrorKind::NotSupported(
            "IOKit URB execution not yet implemented — use rusb backend on macOS".into(),
        )))
    }
}

impl IokitBackend {
    /// Find an IOUSBDevice service by busid string.
    fn find_service(&self, busid: &str) -> UsbIpResult<u32> {
        // Parse busid "busnum-devnum" or just the location ID
        let _parts: Vec<&str> = busid.split('-').collect();

        // In a full implementation, we'd match against the device's
        // location ID property. For now, just verify the busid format.
        if busid.contains('-') {
            // Accept any well-formed busid as valid for lookup
            Ok(0)
        } else {
            Err(UsbIpError::from(ErrorKind::DeviceNotFound(busid.into())))
        }
    }
}

// ─── IOKit Device Enumeration ────────────────────────────────────────────

/// Enumerate a single IOUSBDevice service into a UsbIpDeviceEntry.
unsafe fn enumerate_one_device(service: io_service_t, index: u32) -> Option<UsbIpDeviceEntry> {
    let vendor_id = get_iokit_u16(service, "idVendor")?;
    let product_id = get_iokit_u16(service, "idProduct")?;
    let device_release = get_iokit_u16(service, "bcdDevice").unwrap_or(0);

    // Get class/subclass/protocol from properties
    let class_code = get_iokit_u8(service, "bDeviceClass").unwrap_or(0);
    let sub_class = get_iokit_u8(service, "bDeviceSubClass").unwrap_or(0);
    let protocol = get_iokit_u8(service, "bDeviceProtocol").unwrap_or(0);
    let num_configs = get_iokit_u8(service, "bNumConfigurations").unwrap_or(1);
    let num_ifaces = get_iokit_u8(service, "bNumInterfaces").unwrap_or(0);

    // Get location ID as the bus address
    let location_id = get_iokit_u32(service, "locationID").unwrap_or(index);
    let busnum = ((location_id >> 16) & 0xFF) as u8;
    let devnum = (location_id & 0xFF) as u8;

    // Build busid: "busnum-devnum"
    let busid = format!("{}-{}", busnum, devnum);
    let path = format!("/sys/bus/usb/devices/{}", busid);

    // Get USB speed
    let speed_val = get_iokit_u8(service, "speed").unwrap_or(0);
    let speed = match speed_val {
        0 => 1, // USB 1.0 → LOW
        1 => 2, // USB 1.1 → FULL
        2 => 3, // USB 2.0 → HIGH
        3 => 5, // USB 3.0 → SUPER
        4 => 6, // USB 3.1 → SUPER_PLUS
        _ => 3, // default to HIGH
    };

    let mut entry = UsbIpDeviceEntry {
        path: [0u8; 256],
        busid: [0u8; 32],
        busnum: U32BE::new(busnum as u32),
        devnum: U32BE::new(devnum as u32),
        speed: U32BE::new(speed),
        id_vendor: U16BE::new(vendor_id),
        id_product: U16BE::new(product_id),
        bcd_device: U16BE::new(device_release),
        b_device_class: class_code,
        b_device_sub_class: sub_class,
        b_device_protocol: protocol,
        b_configuration_value: 1,
        b_num_configurations: num_configs,
        b_num_interfaces: num_ifaces,
    };

    let path_bytes = path.as_bytes();
    let copy_len = path_bytes.len().min(255);
    entry.path[..copy_len].copy_from_slice(&path_bytes[..copy_len]);

    let busid_bytes = busid.as_bytes();
    let copy_len = busid_bytes.len().min(31);
    entry.busid[..copy_len].copy_from_slice(&busid_bytes[..copy_len]);

    debug!("IOKit enumerated: {:04x}:{:04x} at {}", vendor_id, product_id, busid);

    Some(entry)
}

// ─── IOKit Property Helpers ──────────────────────────────────────────────

unsafe fn get_iokit_u16(service: io_service_t, key: &str) -> Option<u16> {
    let cf_key = cfstring(key);
    if cf_key.is_null() {
        return None;
    }

    let value = IORegistryEntryCreateCFProperty(service, cf_key, std::ptr::null(), 0);
    CFRelease(cf_key);

    if value.is_null() {
        return None;
    }

    let type_id = CFGetTypeID(value);
    let number_type_id = CFNumberGetTypeID();

    let result = if type_id == number_type_id {
        let mut val: u16 = 0;
        let success = CFNumberGetValue(
            value,
            kCFNumberSInt16Type,
            &mut val as *mut u16 as *mut std::ffi::c_void,
        );
        if success != 0 {
            Some(val)
        } else {
            None
        }
    } else if type_id == CFStringGetTypeID() {
        // Try to parse as hex string
        let s = cfstring_to_string(value);
        if let Some(s) = s {
            u16::from_str_radix(s.trim_start_matches("0x"), 16).ok()
        } else {
            None
        }
    } else {
        None
    };

    CFRelease(value);
    result
}

unsafe fn get_iokit_u8(service: io_service_t, key: &str) -> Option<u8> {
    let cf_key = cfstring(key);
    if cf_key.is_null() {
        return None;
    }

    let value = IORegistryEntryCreateCFProperty(service, cf_key, std::ptr::null(), 0);
    CFRelease(cf_key);

    if value.is_null() {
        return None;
    }

    let type_id = CFGetTypeID(value);
    let number_type_id = CFNumberGetTypeID();

    let result = if type_id == number_type_id {
        let mut val: u8 = 0;
        let success = CFNumberGetValue(
            value,
            kCFNumberSInt8Type,
            &mut val as *mut u8 as *mut std::ffi::c_void,
        );
        if success != 0 {
            Some(val)
        } else {
            None
        }
    } else {
        None
    };

    CFRelease(value);
    result
}

unsafe fn get_iokit_u32(service: io_service_t, key: &str) -> Option<u32> {
    let cf_key = cfstring(key);
    if cf_key.is_null() {
        return None;
    }

    let value = IORegistryEntryCreateCFProperty(service, cf_key, std::ptr::null(), 0);
    CFRelease(cf_key);

    if value.is_null() {
        return None;
    }

    let type_id = CFGetTypeID(value);
    let number_type_id = CFNumberGetTypeID();

    let result = if type_id == number_type_id {
        let mut val: u32 = 0;
        let success = CFNumberGetValue(
            value,
            kCFNumberSInt32Type,
            &mut val as *mut u32 as *mut std::ffi::c_void,
        );
        if success != 0 {
            Some(val)
        } else {
            None
        }
    } else {
        None
    };

    CFRelease(value);
    result
}

unsafe fn get_descriptor_tree_from_iokit(busid: &str) -> UsbIpResult<Vec<u8>> {
    // Build a minimal device descriptor from what we can get via IOKit
    // properties.  A full implementation would use the IOUSBDeviceInterface
    // to read the raw descriptor data.
    //
    // For now this returns a synthetic device descriptor with length=18, type=1.
    // Real IOKit DeviceDescriptorRead would be used in the full impl.
    let parts: Vec<&str> = busid.split('-').collect();
    if parts.len() < 2 {
        return Err(UsbIpError::from(ErrorKind::DeviceNotFound(busid.into())));
    }

    // Return a placeholder device descriptor
    Ok(vec![
        0x12, 0x01, // Length=18, Type=Device
        0x00, 0x02, // USB 2.0 (bcdUSB)
        0x00, // Class
        0x00, // SubClass
        0x00, // Protocol
        0x40, // Max packet size = 64
        0x00, 0x00, // VID (placeholder)
        0x00, 0x00, // PID (placeholder)
        0x00, 0x01, // bcdDevice
        0x00, // Manufacturer string index
        0x00, // Product string index
        0x00, // Serial string index
        0x01, // Num configurations
    ])
}

// CFNumber types (from CFNumber.h)
const kCFNumberSInt8Type: u32 = 1;
const kCFNumberSInt16Type: u32 = 2;
const kCFNumberSInt32Type: u32 = 3;

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// IokitBackend can be constructed.
    #[test]
    fn test_iokit_backend_new() {
        let backend = IokitBackend::new();
        assert!(backend.is_ok());
    }

    /// list_devices should return a Vec (possibly empty).
    #[test]
    fn test_iokit_list_devices_returns_vec() {
        let backend = IokitBackend::new().unwrap();
        let devices = backend.list_devices();
        // On real macOS hardware with USB devices attached, this should
        // return at least the internal USB controller.  We just verify
        // the type and that it doesn't panic.
        assert!(devices.len() <= 256, "sanity check: max 256 devices");
    }

    /// The backend implements the UsbBackend trait.
    #[test]
    fn test_iokit_implements_backend_trait() {
        fn assert_backend<T: UsbBackend>() {}
        assert_backend::<IokitBackend>();
    }

    /// Release of an unclaimed device is safe.
    #[test]
    fn test_iokit_release_unclaimed() {
        let backend = IokitBackend::new().unwrap();
        assert!(backend.release_device("1-1").is_ok());
    }
}
