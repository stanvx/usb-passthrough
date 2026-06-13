//! USB descriptor parsing — device, configuration, interface, endpoint, HID.

/// Standard USB device descriptor (18 bytes).
#[derive(Debug, Clone)]
pub struct DeviceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub bcd_usb: u16,
    pub b_device_class: u8,
    pub b_device_sub_class: u8,
    pub b_device_protocol: u8,
    pub b_max_packet_size0: u8,
    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,
    pub i_manufacturer: u8,
    pub i_product: u8,
    pub i_serial_number: u8,
    pub b_num_configurations: u8,
}

impl DeviceDescriptor {
    pub const SIZE: usize = 18;

    pub fn parse(raw: &[u8]) -> Option<Self> {
        if raw.len() < 18 {
            return None;
        }
        Some(Self {
            b_length: raw[0],
            b_descriptor_type: raw[1],
            bcd_usb: u16::from_le_bytes([raw[2], raw[3]]),
            b_device_class: raw[4],
            b_device_sub_class: raw[5],
            b_device_protocol: raw[6],
            b_max_packet_size0: raw[7],
            id_vendor: u16::from_le_bytes([raw[8], raw[9]]),
            id_product: u16::from_le_bytes([raw[10], raw[11]]),
            bcd_device: u16::from_le_bytes([raw[12], raw[13]]),
            i_manufacturer: raw[14],
            i_product: raw[15],
            i_serial_number: raw[16],
            b_num_configurations: raw[17],
        })
    }
}

/// Standard USB configuration descriptor (9 bytes).
#[derive(Debug, Clone)]
pub struct ConfigDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub w_total_length: u16,
    pub b_num_interfaces: u8,
    pub b_configuration_value: u8,
    pub i_configuration: u8,
    pub bm_attributes: u8,
    pub b_max_power: u8,
}

impl ConfigDescriptor {
    pub const SIZE: usize = 9;

    pub fn parse(raw: &[u8]) -> Option<Self> {
        if raw.len() < 9 || raw[1] != 0x02 {
            return None;
        }
        Some(Self {
            b_length: raw[0],
            b_descriptor_type: raw[1],
            w_total_length: u16::from_le_bytes([raw[2], raw[3]]),
            b_num_interfaces: raw[4],
            b_configuration_value: raw[5],
            i_configuration: raw[6],
            bm_attributes: raw[7],
            b_max_power: raw[8],
        })
    }
}

/// Standard USB interface descriptor (9 bytes).
#[derive(Debug, Clone)]
pub struct InterfaceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_interface_number: u8,
    pub b_alternate_setting: u8,
    pub b_num_endpoints: u8,
    pub b_interface_class: u8,
    pub b_interface_sub_class: u8,
    pub b_interface_protocol: u8,
    pub i_interface: u8,
}

impl InterfaceDescriptor {
    pub const SIZE: usize = 9;

    pub fn parse(raw: &[u8]) -> Option<Self> {
        if raw.len() < 9 || raw[1] != 0x04 {
            return None;
        }
        Some(Self {
            b_length: raw[0],
            b_descriptor_type: raw[1],
            b_interface_number: raw[2],
            b_alternate_setting: raw[3],
            b_num_endpoints: raw[4],
            b_interface_class: raw[5],
            b_interface_sub_class: raw[6],
            b_interface_protocol: raw[7],
            i_interface: raw[8],
        })
    }
}

/// Standard USB endpoint descriptor (7 bytes).
#[derive(Debug, Clone)]
pub struct EndpointDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_endpoint_address: u8,
    pub bm_attributes: u8,
    pub w_max_packet_size: u16,
    pub b_interval: u8,
}

impl EndpointDescriptor {
    pub const SIZE: usize = 7;

    pub fn parse(raw: &[u8]) -> Option<Self> {
        if raw.len() < 7 || raw[1] != 0x05 {
            return None;
        }
        Some(Self {
            b_length: raw[0],
            b_descriptor_type: raw[1],
            b_endpoint_address: raw[2],
            bm_attributes: raw[3],
            w_max_packet_size: u16::from_le_bytes([raw[4], raw[5]]),
            b_interval: raw[6],
        })
    }

    /// Endpoint number (lower 4 bits of address).
    pub fn ep_number(&self) -> u8 {
        self.b_endpoint_address & 0x0F
    }

    /// Direction: true = IN (device→host).
    pub fn is_in(&self) -> bool {
        (self.b_endpoint_address & 0x80) != 0
    }

    /// Transfer type from bm_attributes bits 0-1.
    pub fn transfer_type(&self) -> EndpointType {
        match self.bm_attributes & 0x03 {
            0 => EndpointType::Control,
            1 => EndpointType::Isochronous,
            2 => EndpointType::Bulk,
            3 => EndpointType::Interrupt,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointType {
    Control,
    Isochronous,
    Bulk,
    Interrupt,
}

/// HID descriptor (class-specific, follows interface descriptor).
#[derive(Debug, Clone)]
pub struct HidDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub bcd_hid: u16,
    pub b_country_code: u8,
    pub b_num_descriptors: u8,
    pub report_descriptor_type: u8,
    pub w_report_descriptor_length: u16,
}

impl HidDescriptor {
    pub const SIZE: usize = 9;

    pub fn parse(raw: &[u8]) -> Option<Self> {
        if raw.len() < 9 || raw[1] != 0x21 {
            return None;
        }
        Some(Self {
            b_length: raw[0],
            b_descriptor_type: raw[1],
            bcd_hid: u16::from_le_bytes([raw[2], raw[3]]),
            b_country_code: raw[4],
            b_num_descriptors: raw[5],
            report_descriptor_type: raw[6],
            w_report_descriptor_length: u16::from_le_bytes([raw[7], raw[8]]),
        })
    }
}

/// A fully-parsed USB device with all its descriptors.
#[derive(Debug, Clone)]
pub struct UsbDeviceInfo {
    pub device: DeviceDescriptor,
    pub configs: Vec<ConfigInfo>,
}

#[derive(Debug, Clone)]
pub struct ConfigInfo {
    pub config: ConfigDescriptor,
    pub interfaces: Vec<InterfaceInfo>,
}

#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub interface: InterfaceDescriptor,
    pub endpoints: Vec<EndpointDescriptor>,
    pub hid: Option<HidDescriptor>,
}

impl UsbDeviceInfo {
    /// Parse a raw descriptor tree (as received in OP_REP_IMPORT).
    pub fn parse_descriptor_tree(raw: &[u8]) -> Option<Self> {
        let device = DeviceDescriptor::parse(raw)?;
        let mut configs = Vec::new();
        let mut offset = DeviceDescriptor::SIZE;

        while offset < raw.len() {
            let desc_len = raw[offset] as usize;
            let desc_type = raw[offset + 1];

            match desc_type {
                0x02 => {
                    // Configuration descriptor
                    let config = ConfigDescriptor::parse(&raw[offset..])?;
                    let total_len = config.w_total_length as usize;
                    let mut interfaces = Vec::new();
                    let mut inner_offset = offset + ConfigDescriptor::SIZE;
                    let config_end = offset + total_len.min(raw.len());

                    while inner_offset + InterfaceDescriptor::SIZE <= config_end {
                        let iface = InterfaceDescriptor::parse(&raw[inner_offset..])?;
                        let mut endpoints = Vec::new();
                        let mut hid: Option<HidDescriptor> = None;
                        inner_offset += InterfaceDescriptor::SIZE;

                        // Parse endpoint and HID descriptors within interface
                        for _ in 0..iface.b_num_endpoints + 1 {
                            if inner_offset + 2 > config_end {
                                break;
                            }
                            let sub_type = raw[inner_offset + 1];
                            match sub_type {
                                0x05 => {
                                    if let Some(ep) =
                                        EndpointDescriptor::parse(&raw[inner_offset..])
                                    {
                                        inner_offset += EndpointDescriptor::SIZE;
                                        endpoints.push(ep);
                                        continue;
                                    }
                                },
                                0x21 => {
                                    if let Some(h) = HidDescriptor::parse(&raw[inner_offset..]) {
                                        inner_offset += HidDescriptor::SIZE;
                                        hid = Some(h);
                                        continue;
                                    }
                                },
                                _ => {},
                            }
                            break;
                        }

                        interfaces.push(InterfaceInfo { interface: iface, endpoints, hid });

                        if inner_offset >= config_end {
                            break;
                        }
                    }

                    configs.push(ConfigInfo { config, interfaces });
                    offset = config_end;
                },
                _ => {
                    // Skip unknown descriptors
                    if desc_len == 0 || offset + desc_len > raw.len() {
                        break;
                    }
                    offset += desc_len;
                },
            }
        }

        Some(Self { device, configs })
    }
}
