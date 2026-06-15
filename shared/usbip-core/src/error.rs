//! Structured error types for the USB/IP stack.
//!
//! Every error carries a [`CorrelationId`] (UUIDv7) and an [`ErrorCategory`]
//! that tells the caller whether the failure is transient, permanent, or fatal.
//! This categorisation is the foundation for hot-plug detection and
//! auto-reconnect — see ADR-0003.

use std::collections::HashMap;
use std::fmt;

use uuid::Uuid;

/// The category of a USB/IP error.
///
/// This is the design decision that unlocks ADR-0003: hot-plug detection
/// needs to recognise [`Permanent::DeviceNotFound`], and auto-reconnect
/// needs to recognise [`Transient::Timeout`] / [`Transient::ConnectionClosed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Network blip, timeout — retry is appropriate.
    Transient,
    /// Device removed, protocol violation — retry is futile.
    Permanent,
    /// Corrupt state, invariant broken — caller should abort.
    Fatal,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCategory::Transient => write!(f, "transient"),
            ErrorCategory::Permanent => write!(f, "permanent"),
            ErrorCategory::Fatal => write!(f, "fatal"),
        }
    }
}

/// A time-sortable unique identifier for tracing an error through the system.
///
/// UUIDv7 (time-ordered) so that log lines are naturally sorted by occurrence.
pub type CorrelationId = Uuid;

/// The specific kind of USB/IP error.
///
/// Each variant maps to a canonical [`ErrorCategory`] via [`ErrorKind::category`].
#[derive(thiserror::Error, Debug)]
pub enum ErrorKind {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[cfg(not(target_os = "android"))]
    #[error("USB error: {0}")]
    Usb(#[from] rusb::Error),

    #[cfg(target_os = "android")]
    #[error("USB error: errno={0}")]
    UsbRaw(i32),

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

impl ErrorKind {
    /// Return the canonical [`ErrorCategory`] for this kind.
    pub fn category(&self) -> ErrorCategory {
        match self {
            ErrorKind::Timeout | ErrorKind::ConnectionClosed => ErrorCategory::Transient,

            ErrorKind::Io(e) => match e.kind() {
                std::io::ErrorKind::WouldBlock
                | std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::Interrupted
                | std::io::ErrorKind::NotConnected
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::UnexpectedEof => ErrorCategory::Transient,
                _ => ErrorCategory::Permanent,
            },

            #[cfg(not(target_os = "android"))]
            ErrorKind::Usb(e) => {
                // rusb errors that map to a transport retry
                if matches!(e, rusb::Error::Timeout | rusb::Error::Busy) {
                    ErrorCategory::Transient
                } else {
                    ErrorCategory::Permanent
                }
            },

            #[cfg(target_os = "android")]
            ErrorKind::UsbRaw(_) => ErrorCategory::Permanent,

            ErrorKind::DeviceNotFound(_)
            | ErrorKind::DeviceBusy(_)
            | ErrorKind::NotSupported(_)
            | ErrorKind::Protocol(_)
            | ErrorKind::InvalidMessage(_) => ErrorCategory::Permanent,

            ErrorKind::UrbFailed(_) | ErrorKind::Encryption(_) | ErrorKind::Serialization(_) => {
                ErrorCategory::Fatal
            },
        }
    }
}

/// A structured USB/IP error with correlation ID, category, and arbitrary context.
pub struct UsbIpError {
    kind: ErrorKind,
    category: ErrorCategory,
    correlation_id: CorrelationId,
    context: HashMap<&'static str, String>,
}

impl UsbIpError {
    /// Create a new error with a fresh UUIDv7 correlation ID.
    ///
    /// The category is inferred from `kind.category()` unless overridden
    /// (rare — use `with_category` when the caller knows more context).
    pub fn new(kind: ErrorKind, category: ErrorCategory) -> Self {
        Self { kind, category, correlation_id: Uuid::now_v7(), context: HashMap::new() }
    }

    /// Attach a structured key-value pair to the error.
    ///
    /// `with_context("busid", "3-2")` adds diagnostic detail that
    /// appears in the Display and Debug output.
    pub fn with_context(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.context.insert(key, value.into());
        self
    }

    /// Read a previously-attached context value.
    pub fn get_context(&self, key: &str) -> Option<&String> {
        self.context.get(key)
    }

    /// The specific error kind.
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    /// The error category (transient, permanent, or fatal).
    pub fn category(&self) -> ErrorCategory {
        self.category
    }

    /// The correlation ID for this error instance.
    pub fn correlation_id(&self) -> CorrelationId {
        self.correlation_id
    }
}

impl fmt::Display for UsbIpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] [{}] ", self.category, self.correlation_id)?;
        write!(f, "{}", self.kind)?;
        if !self.context.is_empty() {
            let mut pairs: Vec<_> = self.context.iter().collect();
            pairs.sort_by_key(|(k, _)| *k);
            for (k, v) in &pairs {
                write!(f, " {k}={v}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for UsbIpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UsbIpError")
            .field("kind", &self.kind)
            .field("category", &self.category)
            .field("correlation_id", &self.correlation_id)
            .field("context", &self.context)
            .finish()
    }
}

impl std::error::Error for UsbIpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.kind)
    }
}

// ── From conversions — these let ? work on ErrorKind variants ────────

impl From<ErrorKind> for UsbIpError {
    fn from(kind: ErrorKind) -> Self {
        let category = kind.category();
        Self::new(kind, category)
    }
}

impl From<std::io::Error> for UsbIpError {
    fn from(e: std::io::Error) -> Self {
        let category = match e.kind() {
            std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::UnexpectedEof => ErrorCategory::Transient,
            _ => ErrorCategory::Permanent,
        };
        Self::new(ErrorKind::Io(e), category)
    }
}

#[cfg(not(target_os = "android"))]
impl From<rusb::Error> for UsbIpError {
    fn from(e: rusb::Error) -> Self {
        let category = if matches!(e, rusb::Error::Timeout | rusb::Error::Busy) {
            ErrorCategory::Transient
        } else {
            ErrorCategory::Permanent
        };
        Self::new(ErrorKind::Usb(e), category)
    }
}

/// Result alias for USB/IP operations.
pub type UsbIpResult<T> = Result<T, UsbIpError>;

/// Convert a libusb error to a USB/IP URB status code.
#[cfg(not(target_os = "android"))]
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── ErrorCategory ──────────────────────────────────────

    #[test]
    fn test_error_category_display() {
        assert_eq!(format!("{}", ErrorCategory::Transient), "transient");
        assert_eq!(format!("{}", ErrorCategory::Permanent), "permanent");
        assert_eq!(format!("{}", ErrorCategory::Fatal), "fatal");
    }

    #[test]
    fn test_error_category_debug() {
        assert_eq!(format!("{:?}", ErrorCategory::Transient), "Transient");
        assert_eq!(format!("{:?}", ErrorCategory::Permanent), "Permanent");
        assert_eq!(format!("{:?}", ErrorCategory::Fatal), "Fatal");
    }

    // ── CorrelationId ──────────────────────────────────────

    #[test]
    fn test_correlation_id_is_uuid_v7() {
        let cid = CorrelationId::now_v7();
        // UUIDv7 stores the version nibble at byte 6, bits 7-4.
        assert_eq!(cid.as_bytes()[6] >> 4, 7, "CorrelationId must be UUIDv7");
    }

    #[test]
    fn test_correlation_ids_are_unique() {
        let a = CorrelationId::now_v7();
        let b = CorrelationId::now_v7();
        assert_ne!(a, b, "each call must produce a unique ID");
    }

    #[test]
    fn test_correlation_id_display() {
        let cid = CorrelationId::now_v7();
        let s = cid.to_string();
        assert_eq!(s.len(), 36, "UUIDv7 string is 36 chars");
        assert_eq!(s.chars().filter(|&c| c == '-').count(), 4, "4 hyphens");
    }

    // ── UsbIpError construction ────────────────────────────

    #[test]
    fn test_error_new_produces_correlation_id() {
        let err = UsbIpError::new(ErrorKind::Timeout, ErrorCategory::Transient);
        let cid_str = err.correlation_id().to_string();
        assert_eq!(cid_str.len(), 36);
        assert_eq!(cid_str.chars().filter(|&c| c == '-').count(), 4);
    }

    #[test]
    fn test_error_new_preserves_kind() {
        let err =
            UsbIpError::new(ErrorKind::DeviceNotFound("foo".into()), ErrorCategory::Permanent);
        assert!(matches!(err.kind(), ErrorKind::DeviceNotFound(_)));
    }

    #[test]
    fn test_error_new_preserves_category() {
        let err = UsbIpError::new(ErrorKind::Timeout, ErrorCategory::Transient);
        assert_eq!(err.category(), ErrorCategory::Transient);
    }

    #[test]
    fn test_error_with_context_adds_key_value() {
        let err = UsbIpError::new(ErrorKind::ConnectionClosed, ErrorCategory::Transient)
            .with_context("busid", "3-2")
            .with_context("peer", "192.168.1.5:3240");
        assert_eq!(err.get_context("busid"), Some(&"3-2".to_string()));
        assert_eq!(err.get_context("peer"), Some(&"192.168.1.5:3240".to_string()));
        assert_eq!(err.get_context("nonexistent"), None);
    }

    #[test]
    fn test_error_display_one_line() {
        let err =
            UsbIpError::new(ErrorKind::DeviceNotFound("3-2".into()), ErrorCategory::Permanent);
        let s = err.to_string();
        assert!(s.contains("[permanent]"), "display must contain category");
        assert!(s.contains("Device not found"), "display must contain kind message");
        assert!(s.contains("3-2"), "display must contain context from kind");
    }

    #[test]
    fn test_error_display_includes_correlation_id() {
        let err = UsbIpError::new(ErrorKind::ConnectionClosed, ErrorCategory::Transient);
        let s = err.to_string();
        let expected_cid = err.correlation_id().to_string();
        assert!(s.contains(&expected_cid), "display must include correlation ID");
    }

    // ── ErrorKind → ErrorCategory classification ──────────

    #[test]
    fn test_transient_kinds_are_all_detected() {
        let kinds = vec![
            ErrorKind::Timeout,
            ErrorKind::ConnectionClosed,
            ErrorKind::Io(std::io::Error::new(std::io::ErrorKind::WouldBlock, "test")),
            #[cfg(not(target_os = "android"))]
            ErrorKind::Usb(rusb::Error::Timeout),
        ];
        for kind in kinds {
            let cat = kind.category();
            assert_eq!(cat, ErrorCategory::Transient, "expected transient for {:?}", kind);
        }
    }

    #[test]
    fn test_permanent_kinds_are_all_detected() {
        let kinds = vec![
            ErrorKind::DeviceNotFound("foo".into()),
            ErrorKind::DeviceBusy("foo".into()),
            ErrorKind::NotSupported("foo".into()),
            ErrorKind::Protocol("foo".into()),
            ErrorKind::InvalidMessage("foo".into()),
        ];
        for kind in kinds {
            let cat = kind.category();
            assert_eq!(cat, ErrorCategory::Permanent, "expected permanent for {:?}", kind);
        }
    }

    #[test]
    fn test_fatal_kinds_are_all_detected() {
        let kinds = vec![
            ErrorKind::UrbFailed(0),
            ErrorKind::Encryption("foo".into()),
            ErrorKind::Serialization("foo".into()),
        ];
        for kind in kinds {
            let cat = kind.category();
            assert_eq!(cat, ErrorCategory::Fatal, "expected fatal for {:?}", kind);
        }
    }

    // ── Backward compat: rusb_to_urb_status ────────────────

    #[cfg(not(target_os = "android"))]
    #[test]
    fn test_rusb_to_urb_status_unchanged() {
        assert_eq!(rusb_to_urb_status(&rusb::Error::Io), -5);
        assert_eq!(rusb_to_urb_status(&rusb::Error::InvalidParam), -22);
        assert_eq!(rusb_to_urb_status(&rusb::Error::Access), -1);
        assert_eq!(rusb_to_urb_status(&rusb::Error::NoDevice), -19);
        assert_eq!(rusb_to_urb_status(&rusb::Error::NotFound), -2);
        assert_eq!(rusb_to_urb_status(&rusb::Error::Busy), -16);
        assert_eq!(rusb_to_urb_status(&rusb::Error::Timeout), -62);
        assert_eq!(rusb_to_urb_status(&rusb::Error::Overflow), -75);
        assert_eq!(rusb_to_urb_status(&rusb::Error::Pipe), -32);
        assert_eq!(rusb_to_urb_status(&rusb::Error::Interrupted), -4);
        assert_eq!(rusb_to_urb_status(&rusb::Error::NoMem), -12);
        assert_eq!(rusb_to_urb_status(&rusb::Error::NotSupported), -95);
    }

    // ── Implements std::error::Error (backward compat) ─────

    #[test]
    fn test_error_implements_std_error() {
        let err = UsbIpError::new(ErrorKind::Timeout, ErrorCategory::Transient);
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<UsbIpError>();
    }
}
