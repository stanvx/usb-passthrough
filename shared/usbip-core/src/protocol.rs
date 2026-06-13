//! USB/IP wire protocol constants, header types, and message construction.

use zerocopy::byteorder::{BigEndian, I32, U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Re-export big-endian types for convenience.
pub type U16BE = U16<BigEndian>;
pub type U32BE = U32<BigEndian>;
pub type I32BE = I32<BigEndian>;

// ─── USB/IP Command Codes ────────────────────────────────────────

/// Request device list (client → server).
pub const OP_REQ_DEVLIST: u16 = 0x8003;

/// Reply with device list (server → client).
pub const OP_REP_DEVLIST: u16 = 0x0005;

/// Request to import a device (client → server).
pub const OP_REQ_IMPORT: u16 = 0x8006;

/// Reply to import request (server → client).
pub const OP_REP_IMPORT: u16 = 0x0007;

/// Submit a URB / transfer request (client → server).
pub const USBIP_CMD_SUBMIT: u16 = 0x0001;

/// URB completion / unlink (server → client, async).
pub const USBIP_RET_UNLINK: u16 = 0x0002;

/// URB submission reply (server → client).
pub const USBIP_RET_SUBMIT: u16 = 0x0003;

// ─── Status Codes ────────────────────────────────────────────────

pub const STATUS_SUCCESS: i32 = 0;
pub const STATUS_ST_NA: i32 = 1; // device not available
pub const STATUS_ST_DEV_BUSY: i32 = 2; // device already exported
pub const STATUS_ST_DEV_ERR: i32 = 3; // device error
pub const STATUS_ST_NODEV: i32 = 4; // no such device
pub const STATUS_ST_ERROR: i32 = 5; // generic error

// ─── URB Transfer Flags ──────────────────────────────────────────

pub const URB_SHORT_NOT_OK: u32 = 0x0001;
pub const URB_ISO_ASAP: u32 = 0x0002;
pub const URB_NO_TRANSFER_DMA_MAP: u32 = 0x0004;
pub const URB_ZERO_PACKET: u32 = 0x0040;
pub const URB_NO_INTERRUPT: u32 = 0x0080;
pub const URB_FREE_BUFFER: u32 = 0x0100;
pub const URB_DIR_IN: u32 = 0x0200;
pub const URB_DIR_OUT: u32 = 0;

// ─── Wire-Format Structs ─────────────────────────────────────────

/// The 8-byte header that starts every USB/IP message.
#[derive(Debug, Clone, IntoBytes, FromBytes, KnownLayout, Immutable)]
#[repr(C, packed)]
pub struct UsbIpHeader {
    pub version: U16BE,
    pub command: U16BE,
    pub status: I32BE,
}

impl UsbIpHeader {
    pub const SIZE: usize = 8;

    pub fn new(command: u16) -> Self {
        Self {
            version: U16BE::new(crate::USBIP_VERSION),
            command: U16BE::new(command),
            status: I32BE::new(0),
        }
    }

    pub fn with_status(command: u16, status: i32) -> Self {
        Self {
            version: U16BE::new(crate::USBIP_VERSION),
            command: U16BE::new(command),
            status: I32BE::new(status),
        }
    }
}

/// A single device entry in OP_REP_DEVLIST (312 bytes).
#[derive(Debug, Clone, IntoBytes, FromBytes, KnownLayout, Immutable)]
#[repr(C, packed)]
pub struct UsbIpDeviceEntry {
    pub path: [u8; 256],
    pub busid: [u8; 32],
    pub busnum: U32BE,
    pub devnum: U32BE,
    pub speed: U32BE,
    pub id_vendor: U16BE,
    pub id_product: U16BE,
    pub bcd_device: U16BE,
    pub b_device_class: u8,
    pub b_device_sub_class: u8,
    pub b_device_protocol: u8,
    pub b_configuration_value: u8,
    pub b_num_configurations: u8,
    pub b_num_interfaces: u8,
}

impl UsbIpDeviceEntry {
    pub const SIZE: usize = 312;

    /// Read the busid as a string (null-terminated or full buffer).
    pub fn busid_str(&self) -> &str {
        let end = self.busid.iter().position(|&b| b == 0).unwrap_or(32);
        core::str::from_utf8(&self.busid[..end]).unwrap_or("???")
    }

    /// Read the path as a string.
    pub fn path_str(&self) -> &str {
        let end = self.path.iter().position(|&b| b == 0).unwrap_or(256);
        core::str::from_utf8(&self.path[..end]).unwrap_or("???")
    }

    pub fn vid(&self) -> u16 {
        self.id_vendor.get()
    }
    pub fn pid(&self) -> u16 {
        self.id_product.get()
    }
    pub fn speed_val(&self) -> u32 {
        self.speed.get()
    }
}

/// The variable-length portion of OP_REP_IMPORT: device entry + descriptor tree.
#[derive(Debug, Clone)]
pub struct UsbIpImportReply {
    pub device: UsbIpDeviceEntry,
    pub descriptors: Vec<u8>, // raw USB descriptor tree
}
