//! Windows USB device enumeration via Win32 SetupAPI.
//!
//! Uses the Windows SetupAPI (`setupapi.dll`) to enumerate all USB devices
//! present on the system. Returns structured device information compatible
//! with the `usbip-core` shared type system (VID, PID, bus path, speed).
//!

use std::ffi::OsString;
use std::mem;
use std::os::windows::ffi::OsStringExt;
use std::ptr;

use winapi::shared::minwindef::{DWORD, FALSE, TRUE};
use winapi::shared::ntdef::HANDLE;
use winapi::shared::usbiodef::GUID_DEVINTERFACE_USB_DEVICE;
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::setupapi::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceRegistryPropertyW, DIGCF_DEVICEINTERFACE,
    DIGCF_PRESENT, SPDRP_DEVICEDESC, SPDRP_HARDWAREID, SP_DEVINFO_DATA,
};

use usbip_core::error::*;

// ---------------------------------------------------------------------------
// Device Info
// ---------------------------------------------------------------------------

/// Information about a single USB device discovered via SetupAPI.
#[derive(Debug, Clone)]
pub struct UsbDeviceInfo {
    /// USB vendor ID
    pub vendor_id: u16,
    /// USB product ID
    pub product_id: u16,
    /// Device description string (from registry)
    pub description: String,
    /// Instance ID (e.g., "USB\\VID_046D&PID_C261\\123456")
    pub instance_id: String,
    /// Hardware ID (e.g., "USB\\VID_046D&PID_C261&REV_0100")
    pub hardware_id: String,
    /// USB device speed (0=Unknown, 1=Low, 2=Full, 3=High, 4=Super)
    pub speed: u32,
    // Device-specific detection: VID/PID comparison is caller logic.
}

impl Default for UsbDeviceInfo {
    fn default() -> Self {
        Self {
            vendor_id: 0,
            product_id: 0,
            description: String::new(),
            instance_id: String::new(),
            hardware_id: String::new(),
            speed: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Main enumeration function
// ---------------------------------------------------------------------------

/// Enumerate all USB devices present on the system using SetupAPI.
///
/// Returns a vector of [`UsbDeviceInfo`] structs representing every
/// connected USB device. Callers can filter by VID/PID as needed.
pub fn enumerate_usb_devices() -> UsbIpResult<Vec<UsbDeviceInfo>> {
    let mut devices: Vec<UsbDeviceInfo> = Vec::new();

    // SAFETY: Win32 SetupAPI is unsafe; we pass valid pointers and check return values.
    let dev_info_set = unsafe {
        SetupDiGetClassDevsW(
            &GUID_DEVINTERFACE_USB_DEVICE as *const _ as *mut _,
            ptr::null_mut(),
            ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };

    if dev_info_set == INVALID_HANDLE_VALUE {
        return Err(UsbIpError::from(ErrorKind::NotSupported(
            "SetupDiGetClassDevsW failed: no USB device info set".to_string(),
        )));
    }

    // Enumerate devices
    let mut device_index: DWORD = 0;
    loop {
        let mut dev_info_data: SP_DEVINFO_DATA = unsafe { mem::zeroed() };
        dev_info_data.cbSize = mem::size_of::<SP_DEVINFO_DATA>() as DWORD;

        let found =
            unsafe { SetupDiEnumDeviceInfo(dev_info_set, device_index, &mut dev_info_data) };

        if found == FALSE {
            // No more devices
            break;
        }

        let mut info = UsbDeviceInfo::default();

        // Get the device instance ID
        if let Ok(instance_id) = get_device_instance_id(dev_info_set, &dev_info_data) {
            info.instance_id = instance_id;
        }

        // Get the device description
        if let Ok(desc) = get_device_registry_string(dev_info_set, &dev_info_data, SPDRP_DEVICEDESC)
        {
            info.description = desc;
        }

        // Get the hardware ID (contains VID/PID)
        if let Ok(hw_id) =
            get_device_registry_string(dev_info_set, &dev_info_data, SPDRP_HARDWAREID)
        {
            info.hardware_id = hw_id.clone();
            // Parse VID/PID from hardware ID
            parse_vid_pid_from_hardware_id(&hw_id, &mut info.vendor_id, &mut info.product_id);
        }

        // Skip devices with no VID/PID (root hubs, etc.)
        if info.vendor_id != 0 || info.product_id != 0 {
            devices.push(info);
        }

        device_index += 1;
    }

    // SAFETY: Deallocate the device info set we created above.
    unsafe {
        SetupDiDestroyDeviceInfoList(dev_info_set);
    }

    Ok(devices)
}

// ---------------------------------------------------------------------------
// SetupAPI helpers
// ---------------------------------------------------------------------------

/// Get the device instance ID string from SetupAPI.
fn get_device_instance_id(
    dev_info_set: HANDLE,
    dev_info_data: &SP_DEVINFO_DATA,
) -> UsbIpResult<String> {
    let mut required_size: DWORD = 0;

    // First call to get required buffer size (Expected to fail with ERROR_INSUFFICIENT_BUFFER)
    unsafe {
        SetupDiGetDeviceInstanceIdW(
            dev_info_set,
            dev_info_data as *const SP_DEVINFO_DATA as *mut SP_DEVINFO_DATA,
            ptr::null_mut(),
            0,
            &mut required_size,
        )
    };

    let mut buffer = vec![0u16; required_size as usize];
    let result = unsafe {
        SetupDiGetDeviceInstanceIdW(
            dev_info_set,
            dev_info_data as *const SP_DEVINFO_DATA as *mut SP_DEVINFO_DATA,
            buffer.as_mut_ptr(),
            required_size,
            &mut required_size,
        )
    };

    if result == FALSE {
        return Err(UsbIpError::from(ErrorKind::InvalidMessage(
            "SetupDiGetDeviceInstanceIdW failed".to_string(),
        )));
    }

    // Find the null terminator and convert
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let os_string = OsString::from_wide(&buffer[..len]);
    Ok(os_string.to_string_lossy().to_string())
}

/// Get a registry string property for a device.
fn get_device_registry_string(
    dev_info_set: HANDLE,
    dev_info_data: &SP_DEVINFO_DATA,
    property: DWORD,
) -> UsbIpResult<String> {
    let mut required_size: DWORD = 0;
    let mut data_type: DWORD = 0;

    // First call to get required buffer size (Expected to fail with ERROR_INSUFFICIENT_BUFFER)
    unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            dev_info_set,
            dev_info_data as *const SP_DEVINFO_DATA as *mut SP_DEVINFO_DATA,
            property,
            &mut data_type,
            ptr::null_mut(),
            0,
            &mut required_size,
        )
    };

    let mut buffer = vec![0u8; required_size as usize];
    let result = unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            dev_info_set,
            dev_info_data as *const SP_DEVINFO_DATA as *mut SP_DEVINFO_DATA,
            property,
            &mut data_type,
            buffer.as_mut_ptr(),
            required_size as DWORD,
            &mut required_size,
        )
    };

    if result == FALSE {
        return Err(UsbIpError::from(ErrorKind::InvalidMessage(
            "SetupDiGetDeviceRegistryPropertyW failed".to_string(),
        )));
    }

    // Registry strings are REG_SZ (null-terminated UTF-16LE)
    if data_type == winapi::um::winnt::REG_SZ {
        let u16_slice = unsafe {
            std::slice::from_raw_parts(buffer.as_ptr() as *const u16, required_size as usize / 2)
        };
        let len = u16_slice.iter().position(|&c| c == 0).unwrap_or(u16_slice.len());
        let os_string = OsString::from_wide(&u16_slice[..len]);
        Ok(os_string.to_string_lossy().to_string())
    } else {
        // Fallback: try ASCII
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Ok(String::from_utf8_lossy(&buffer[..len]).to_string())
    }
}

// ---------------------------------------------------------------------------
// VID/PID parsing
// ---------------------------------------------------------------------------

/// Parse VID and PID from a USB hardware ID string.
///
/// Typical format: `USB\\VID_046D&PID_C261&REV_0100`
fn parse_vid_pid_from_hardware_id(hw_id: &str, vid: &mut u16, pid: &mut u16) {
    // Try to find "VID_" pattern
    if let Some(vid_start) = hw_id.find("VID_") {
        let vid_str = &hw_id[vid_start + 4..vid_start + 8];
        if let Ok(v) = u16::from_str_radix(vid_str, 16) {
            *vid = v;
        }
    }

    // Try to find "PID_" pattern
    if let Some(pid_start) = hw_id.find("PID_") {
        let pid_str = &hw_id[pid_start + 4..pid_start + 8];
        if let Ok(p) = u16::from_str_radix(pid_str, 16) {
            *pid = p;
        }
    }
}

// ---------------------------------------------------------------------------
// Conversion to usbip-core types
// ---------------------------------------------------------------------------

/// Build a [`usbip_core::protocol::UsbIpDeviceEntry`] from a [`UsbDeviceInfo`].
pub fn to_usbip_device_entry(info: &UsbDeviceInfo) -> usbip_core::protocol::UsbIpDeviceEntry {
    use usbip_core::protocol::{U16BE, U32BE};
    usbip_core::protocol::UsbIpDeviceEntry {
        path: {
            let mut p = [0u8; 256];
            let bytes = info.instance_id.as_bytes();
            let len = bytes.len().min(255);
            p[..len].copy_from_slice(&bytes[..len]);
            p
        },
        busid: {
            let mut b = [0u8; 32];
            // Build busid from last segment of instance_id, e.g. "1-2.3"
            let id = info.instance_id.rsplit('\\').next().unwrap_or("0-0");
            let bytes = id.as_bytes();
            let len = bytes.len().min(31);
            b[..len].copy_from_slice(&bytes[..len]);
            b
        },
        busnum: U32BE::new(0),
        devnum: U32BE::new(0),
        speed: U32BE::new(info.speed),
        id_vendor: U16BE::new(info.vendor_id),
        id_product: U16BE::new(info.product_id),
        bcd_device: U16BE::new(0),
        b_device_class: 0,
        b_device_sub_class: 0,
        b_device_protocol: 0,
        b_configuration_value: 0,
        b_num_configurations: 0,
        b_num_interfaces: 0,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vid_pid() {
        let hw_id = "USB\\VID_046D&PID_C261&REV_0100";
        let mut vid = 0u16;
        let mut pid = 0u16;
        parse_vid_pid_from_hardware_id(hw_id, &mut vid, &mut pid);
        assert_eq!(vid, 0x046D);
        assert_eq!(pid, 0xC261);
    }

    #[test]
    fn test_parse_vid_pid_pc() {
        let hw_id = "USB\\VID_046D&PID_C262&REV_0100";
        let mut vid = 0u16;
        let mut pid = 0u16;
        parse_vid_pid_from_hardware_id(hw_id, &mut vid, &mut pid);
        assert_eq!(vid, 0x046D);
        assert_eq!(pid, 0xC262);
    }

    #[test]
    fn test_parse_vid_pid_reverse_order() {
        // Some hardware IDs have PID before VID
        let hw_id = "USB\\PID_C261&VID_046D";
        let mut vid = 0u16;
        let mut pid = 0u16;
        parse_vid_pid_from_hardware_id(hw_id, &mut vid, &mut pid);
        assert_eq!(vid, 0x046D);
        assert_eq!(pid, 0xC261);
    }

    #[test]
    fn test_enumerate_no_panic() {
        // Should not crash even if called without USB devices
        // (will return empty or populated list depending on test environment)
        let result = enumerate_usb_devices();
        assert!(result.is_ok());
    }
}
