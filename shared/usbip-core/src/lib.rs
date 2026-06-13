//! USB/IP protocol core types, serialization, and constants.
//!
//! Implements the USB/IP protocol as documented in the Linux kernel
//! (Documentation/usb/usbip_protocol.rst). All wire-format types are
//! `#[repr(C, packed)]` for safe transmutation from raw bytes.
//!
//! ## Endianness
//!
//! The USB/IP protocol header uses **big-endian** (network byte order)
//! for multi-byte integer fields. However, the encapsulated USB payload
//! is **little-endian** (USB native). This crate handles both correctly:
//! header fields use `U16BE` / `U32BE` / `I32BE`, while USB descriptor
//! payloads use native `u16` / `u32` (which is LE on all our targets).

pub mod crypto;
pub mod descriptor;
pub mod error;
pub mod pool;
pub mod protocol;
pub mod urb;

pub use crypto::*;
pub use descriptor::*;
pub use error::*;
pub use pool::*;
pub use protocol::*;
pub use urb::*;

/// Default USB/IP TCP port (IANA-registered).
pub const USBIP_PORT: u16 = 3240;

/// USB/IP protocol version we speak.
pub const USBIP_VERSION: u16 = 0x0111; // v1.1.1

/// Maximum size of a single USB/IP message (header + payload).
/// Accommodates SuperSpeed bulk transfers (1024 * 16 packets + overhead).
pub const MAX_MESSAGE_SIZE: usize = 1_048_576; // 1 MiB

/// Maximum number of devices in a devlist reply.
pub const MAX_DEVICES: u32 = 256;

/// USB device speeds (as used in USB/IP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum UsbSpeed {
    Unknown = 0,
    Low = 1,
    Full = 2,
    High = 3,
    Wireless = 4,
    Super = 5,
    SuperPlus = 6,
}

impl UsbSpeed {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Low,
            2 => Self::Full,
            3 => Self::High,
            4 => Self::Wireless,
            5 => Self::Super,
            6 => Self::SuperPlus,
            _ => Self::Unknown,
        }
    }
}

/// USB transfer direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Out = 0, // Host → Device
    In = 1,  // Device → Host
}

impl Direction {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Out,
            _ => Self::In,
        }
    }
}
