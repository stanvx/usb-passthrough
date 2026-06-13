//! Error types for the USB/IP stack.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum UsbIpError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("USB error: {0}")]
    Usb(#[from] rusb::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Connection closed by peer")]
    ConnectionClosed,

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Device busy: {0}")]
    DeviceBusy(String),

    #[error("Device error (URB failed): status={0}")]
    UrbFailed(u32),

    #[error("Operation timed out")]
    Timeout,

    #[error("Not supported: {0}")]
    NotSupported(String),

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Result alias for USB/IP operations.
pub type UsbIpResult<T> = Result<T, UsbIpError>;

/// Convert a libusb error to a USB/IP URB status code.
pub fn rusb_to_urb_status(err: &rusb::Error) -> i32 {
    use rusb::Error;
    match err {
        Error::Io => -5,            // -EIO
        Error::InvalidParam => -22, // -EINVAL
        Error::Access => -1,        // -EPERM
        Error::NoDevice => -19,     // -ENODEV
        Error::NotFound => -2,      // -ENOENT
        Error::Busy => -16,         // -EBUSY
        Error::Timeout => -62,      // -ETIME
        Error::Overflow => -75,     // -EOVERFLOW
        Error::Pipe => -32,         // -EPIPE
        Error::Interrupted => -4,   // -EINTR
        Error::NoMem => -12,        // -ENOMEM
        Error::NotSupported => -95, // -EOPNOTSUPP
        Error::Other => -5,         // -EIO
        _ => -5,
    }
}
