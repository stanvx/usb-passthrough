//! URB executor — the seam between wire-format parsing and physical USB I/O.
//!
//! `UrbExecutor` takes a parsed `UsbIpCmdSubmit` + optional OUT data, delegates
//! to `UsbDeviceManager::execute_urb()`, maps errors, and builds wire-ready
//! `UsbIpRetSubmit` replies.  The wire layer (`server.rs`) stays thin.

use std::sync::Arc;

use usbip_core::error::*;
use usbip_core::protocol::*;
use usbip_core::urb::*;
// U32BE is re-exported through both protocol and urb; import from protocol
// explicitly to avoid the ambiguous-glob-reexport warning.
use usbip_core::protocol::U32BE;
use zerocopy::{FromBytes, IntoBytes};

use crate::usb::UsbDeviceManager;

/// Result of a single URB execution.
///
/// This is **not** a `Result` — even error outcomes produce a valid wire reply
/// (the caller serialises it via `UrbExecutor::build_reply`).
#[derive(Debug, Clone)]
pub struct UrbResult {
    pub status: i32,
    pub actual_length: u32,
    pub data: Vec<u8>,
}

/// Executes URBs against a specific claimed USB device.
pub struct UrbExecutor {
    usb: Arc<UsbDeviceManager>,
    busid: String,
}

impl UrbExecutor {
    pub fn new(usb: Arc<UsbDeviceManager>, busid: String) -> Self {
        Self { usb, busid }
    }

    /// Execute a parsed URB command with optional OUT data.
    ///
    /// Returns an `UrbResult` that the caller serialises into `UsbIpRetSubmit`.
    /// Errors are mapped to negative URB status codes so the caller can always
    /// produce a valid wire reply.
    pub fn execute(&self, cmd: &UsbIpCmdSubmit, out_data: &[u8]) -> UrbResult {
        match self.usb.execute_urb(&self.busid, cmd, out_data) {
            Ok((status, actual_len, in_data)) => {
                UrbResult { status, actual_length: actual_len, data: in_data }
            },
            Err(e) => {
                let urb_status = match e.kind() {
                    ErrorKind::Usb(ref rusb_err) => rusb_to_urb_status(rusb_err),
                    ErrorKind::DeviceNotFound(_) => -19, // -ENODEV
                    ErrorKind::Timeout => -62,           // -ETIME
                    _ => -5,                             // -EIO
                };
                UrbResult { status: urb_status, actual_length: 0, data: Vec::new() }
            },
        }
    }

    /// Build a wire-ready `USBIP_RET_SUBMIT` reply from a command and result.
    pub fn build_reply(&self, cmd: &UsbIpCmdSubmit, result: &UrbResult) -> Vec<u8> {
        let ret = UsbIpRetSubmit {
            seqnum: cmd.seqnum,
            devid: cmd.devid,
            direction: cmd.direction,
            ep: cmd.ep,
            status: U32BE::new(result.status as u32),
            actual_length: U32BE::new(result.actual_length),
            start_frame: cmd.start_frame,
            number_of_packets: cmd.number_of_packets,
            error_count: U32BE::new(if result.status == 0 { 0 } else { 1 }),
            setup: cmd.setup,
        };

        let mut reply = Vec::new();
        let ret_header = UsbIpHeader::new(USBIP_RET_SUBMIT);
        reply.extend_from_slice(ret_header.as_bytes());
        reply.extend_from_slice(ret.as_bytes());
        if !result.data.is_empty() {
            reply.extend_from_slice(&result.data);
        }
        reply
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use usbip_core::protocol::UsbIpHeader;
    use usbip_core::urb::UsbIpRetSubmit;

    // ── helpers ──────────────────────────────────────────────────────────

    /// Build a synthetic `UsbIpCmdSubmit` for testing.
    fn make_cmd(
        endpoint: u32,
        direction: u32,
        transfer_buffer_length: u32,
        setup: [u8; 8],
    ) -> UsbIpCmdSubmit {
        let flags = if direction == 1 { URB_DIR_IN } else { URB_DIR_OUT };
        UsbIpCmdSubmit {
            seqnum: U32BE::new(1),
            devid: U32BE::new(1),
            direction: U32BE::new(direction),
            ep: U32BE::new(endpoint),
            transfer_flags: U32BE::new(flags),
            transfer_buffer_length: U32BE::new(transfer_buffer_length),
            start_frame: U32BE::new(0),
            number_of_packets: U32BE::new(0),
            interval: U32BE::new(0),
            setup,
        }
    }

    /// Check that `make_cmd` produces a well-formed IN command.
    #[test]
    fn test_make_cmd_in() {
        let cmd = make_cmd(0x81, 1, 64, [0u8; 8]);
        assert!(cmd.is_in());
        assert_eq!(cmd.ep_num(), 0x81);
        assert_eq!(cmd.data_len(), 64);
    }

    /// Check that `make_cmd` produces a well-formed OUT command.
    #[test]
    fn test_make_cmd_out() {
        let cmd = make_cmd(0x02, 0, 512, [0u8; 8]);
        assert!(!cmd.is_in());
        assert_eq!(cmd.ep_num(), 0x02);
        assert_eq!(cmd.data_len(), 512);
    }

    // ── UrbResult construction ───────────────────────────────────────────

    #[test]
    fn test_urb_result_ok_in() {
        let result = UrbResult { status: 0, actual_length: 64, data: vec![0xAB; 64] };
        assert_eq!(result.status, 0);
        assert_eq!(result.actual_length, 64);
        assert_eq!(result.data.len(), 64);
    }

    #[test]
    fn test_urb_result_ok_out() {
        let result = UrbResult {
            status: 0,
            actual_length: 64,
            data: Vec::new(), // OUT transfers have no data in the reply
        };
        assert_eq!(result.status, 0);
        assert_eq!(result.actual_length, 64);
        assert!(result.data.is_empty());
    }

    #[test]
    fn test_urb_result_error() {
        let result = UrbResult { status: -5, actual_length: 0, data: Vec::new() };
        assert_eq!(result.status, -5);
        assert_eq!(result.actual_length, 0);
        assert!(result.data.is_empty());
    }

    // ── build_reply ──────────────────────────────────────────────────────

    /// Since `UrbExecutor::new()` needs a live libusb context (which may not
    /// be available on macOS or CI), we construct it inside a helper that we
    /// skip when a real context is unavailable.  All the interesting logic
    /// lives in `build_reply` which only reads `self.busid` (unused) and
    /// `self.usb` (also unused for reply building).

    fn executor() -> Option<UrbExecutor> {
        match UsbDeviceManager::new() {
            Ok(mgr) => {
                let usb = Arc::new(mgr);
                Some(UrbExecutor::new(usb, "1-1".into()))
            },
            Err(_) => {
                tracing::warn!("libusb context unavailable — skipping executor tests");
                None
            },
        }
    }

    // ── build_reply byte-level assertions ──────────────────────────────────
    //
    // These tests verify build_reply output by checking key byte positions
    // rather than round-tripping through zerocopy parsing (which would couple
    // the tests to zerocopy's public API surface).

    /// Read a big-endian u16 from bytes at offset.
    fn be_u16(bytes: &[u8], offset: usize) -> u16 {
        u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
    }

    /// Read a big-endian u32 from bytes at offset.
    fn be_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_be_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]])
    }

    #[test]
    fn test_build_reply_success_in() {
        let Some(ex) = executor() else { return };

        let cmd = make_cmd(0x81, 1, 64, [0u8; 8]);
        let result = UrbResult { status: 0, actual_length: 64, data: vec![0xCD; 64] };

        let reply = ex.build_reply(&cmd, &result);

        // Header (8) + RetSubmit + data (64)
        let expected_len = UsbIpHeader::SIZE + UsbIpRetSubmit::HEADER_SIZE + 64;
        assert_eq!(reply.len(), expected_len);

        // Header: version (u16 BE at offset 0), command (u16 BE at offset 2)
        assert_eq!(be_u16(&reply, 2), USBIP_RET_SUBMIT, "command at offset 2");

        // RetSubmit: seqnum (u32 BE at offset 8) should be 1
        assert_eq!(be_u32(&reply, 8), 1);
        // RetSubmit: status (u32 BE at offset 24) should be 0
        assert_eq!(be_u32(&reply, 24), 0);
        // RetSubmit: actual_length (u32 BE at offset 28) should be 64
        assert_eq!(be_u32(&reply, 28), 64);
        // RetSubmit: error_count (u32 BE at offset 40) should be 0
        assert_eq!(be_u32(&reply, 40), 0);

        // Data payload starts at offset (8 + HEADER_SIZE)
        assert_eq!(&reply[UsbIpHeader::SIZE + UsbIpRetSubmit::HEADER_SIZE..], &[0xCD; 64]);
    }

    #[test]
    fn test_build_reply_success_out() {
        let Some(ex) = executor() else { return };

        let cmd = make_cmd(0x02, 0, 512, [0u8; 8]);
        let result = UrbResult {
            status: 0,
            actual_length: 512,
            data: Vec::new(), // no payload for OUT replies
        };

        let reply = ex.build_reply(&cmd, &result);

        // Header (8) + RetSubmit — no trailing data
        let expected_len = UsbIpHeader::SIZE + UsbIpRetSubmit::HEADER_SIZE;
        assert_eq!(reply.len(), expected_len);

        assert_eq!(be_u16(&reply, 2), USBIP_RET_SUBMIT, "command at offset 2");
        assert_eq!(be_u32(&reply, 8), 1);
        assert_eq!(be_u32(&reply, 24), 0);
        assert_eq!(be_u32(&reply, 28), 512);
        assert_eq!(be_u32(&reply, 40), 0);
    }

    #[test]
    fn test_build_reply_error() {
        let Some(ex) = executor() else { return };

        let cmd = make_cmd(0x81, 1, 64, [0u8; 8]);
        let result = UrbResult { status: -5, actual_length: 0, data: Vec::new() };

        let reply = ex.build_reply(&cmd, &result);

        let expected_len = UsbIpHeader::SIZE + UsbIpRetSubmit::HEADER_SIZE;
        assert_eq!(reply.len(), expected_len); // no data payload

        assert_eq!(be_u16(&reply, 2), USBIP_RET_SUBMIT, "command at offset 2");
        assert_eq!(be_u32(&reply, 8), 1);
        // status = -5, stored as u32 BE with wrapping: 0xFFFFFFFB
        assert_eq!(be_u32(&reply, 24), (-5i32) as u32);
        assert_eq!(be_u32(&reply, 28), 0);
        // error_count = 1 for non-zero status
        assert_eq!(be_u32(&reply, 40), 1);
    }

    #[test]
    fn test_build_reply_zero_length_in() {
        let Some(ex) = executor() else { return };

        // Zero-length IN — valid for some control transfers that only care
        // about the status stage.
        let cmd = make_cmd(0x00, 1, 0, [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]);
        let result = UrbResult { status: 0, actual_length: 0, data: Vec::new() };

        let reply = ex.build_reply(&cmd, &result);

        let expected_len = UsbIpHeader::SIZE + UsbIpRetSubmit::HEADER_SIZE;
        assert_eq!(reply.len(), expected_len); // only header + RetSubmit, no data

        assert_eq!(be_u16(&reply, 2), USBIP_RET_SUBMIT, "command at offset 2");
        assert_eq!(be_u32(&reply, 24), 0);
        assert_eq!(be_u32(&reply, 28), 0);
        assert_eq!(be_u32(&reply, 40), 0);
    }
}
