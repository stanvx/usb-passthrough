//! USB Request Block (URB) types — the core data-transfer primitives.

use zerocopy::byteorder::{BigEndian, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

pub type U32BE = U32<BigEndian>;

/// USBIP_CMD_SUBMIT — Client requests a USB transfer.
///
/// Wire format (80 bytes + variable data for OUT transfers):
#[derive(Debug, Clone, IntoBytes, FromBytes, KnownLayout, Immutable)]
#[repr(C, packed)]
pub struct UsbIpCmdSubmit {
    pub seqnum: U32BE,
    pub devid: U32BE,
    pub direction: U32BE,
    pub ep: U32BE,
    pub transfer_flags: U32BE,
    pub transfer_buffer_length: U32BE,
    pub start_frame: U32BE,
    pub number_of_packets: U32BE,
    pub interval: U32BE,
    pub setup: [u8; 8],
    // Variable-length data follows for OUT transfers
}

impl UsbIpCmdSubmit {
    pub const HEADER_SIZE: usize = 48; // 12 * 4 bytes

    pub fn seqnum(&self) -> u32 {
        self.seqnum.get()
    }
    pub fn devid(&self) -> u32 {
        self.devid.get()
    }
    pub fn dir(&self) -> u32 {
        self.direction.get()
    }
    pub fn ep_num(&self) -> u32 {
        self.ep.get()
    }
    pub fn flags(&self) -> u32 {
        self.transfer_flags.get()
    }
    pub fn data_len(&self) -> u32 {
        self.transfer_buffer_length.get()
    }
    pub fn interval_val(&self) -> u32 {
        self.interval.get()
    }

    /// Returns true if this is an IN (device→host) transfer.
    pub fn is_in(&self) -> bool {
        (self.transfer_flags.get() & crate::protocol::URB_DIR_IN) != 0
    }

    /// Returns true if the setup packet is non-zero (control transfer).
    pub fn is_control(&self) -> bool {
        self.setup != [0u8; 8]
    }

    /// Total wire size including data for OUT transfers.
    pub fn wire_size(&self) -> usize {
        let base = Self::HEADER_SIZE;
        if !self.is_in() {
            base + self.data_len() as usize
        } else {
            base
        }
    }
}

/// USBIP_RET_SUBMIT — Server responds to a URB submission.
#[derive(Debug, Clone, IntoBytes, FromBytes, KnownLayout, Immutable)]
#[repr(C, packed)]
pub struct UsbIpRetSubmit {
    pub seqnum: U32BE,
    pub devid: U32BE,
    pub direction: U32BE,
    pub ep: U32BE,
    pub status: U32BE,
    pub actual_length: U32BE,
    pub start_frame: U32BE,
    pub number_of_packets: U32BE,
    pub error_count: U32BE,
    pub setup: [u8; 8],
    // Variable-length data follows for IN transfers
}

impl UsbIpRetSubmit {
    pub const HEADER_SIZE: usize = 44;

    pub fn seqnum(&self) -> u32 {
        self.seqnum.get()
    }
    pub fn devid(&self) -> u32 {
        self.devid.get()
    }
    pub fn status_val(&self) -> u32 {
        self.status.get()
    }
    pub fn actual_len(&self) -> u32 {
        self.actual_length.get()
    }
    pub fn dir(&self) -> u32 {
        self.direction.get()
    }

    pub fn is_success(&self) -> bool {
        self.status.get() == 0
    }

    /// Returns true if data follows the header (successful IN transfer).
    pub fn has_data(&self) -> bool {
        self.is_success()
            && (self.direction.get() & crate::protocol::URB_DIR_IN) != 0
            && self.actual_length.get() > 0
    }

    /// Total wire size including data for IN transfers.
    pub fn wire_size(&self) -> usize {
        let base = Self::HEADER_SIZE;
        if self.has_data() {
            base + self.actual_len() as usize
        } else {
            base
        }
    }
}

/// USBIP_RET_UNLINK — Server notifies client that a URB was cancelled.
#[derive(Debug, Clone, IntoBytes, FromBytes, KnownLayout, Immutable)]
#[repr(C, packed)]
pub struct UsbIpRetUnlink {
    pub seqnum: U32BE,
    pub devid: U32BE,
    pub status: U32BE,
}

impl UsbIpRetUnlink {
    pub const SIZE: usize = 12;

    pub fn seqnum(&self) -> u32 {
        self.seqnum.get()
    }
    pub fn devid(&self) -> u32 {
        self.devid.get()
    }
    pub fn status_val(&self) -> u32 {
        self.status.get()
    }
}

// ─── Convenience Types (not wire-format) ────────────────────────

/// A complete USB/IP message (header + body) for async processing.
#[derive(Debug)]
pub struct UsbIpMessage {
    pub header: super::protocol::UsbIpHeader,
    /// Raw payload bytes (all data after the 8-byte header).
    pub payload: Vec<u8>,
}

impl UsbIpMessage {
    /// Parse a CMD_SUBMIT from the payload.
    pub fn as_cmd_submit(&self) -> Option<&UsbIpCmdSubmit> {
        if self.header.command.get() != super::protocol::USBIP_CMD_SUBMIT {
            return None;
        }
        UsbIpCmdSubmit::ref_from_prefix(&self.payload).ok().map(|(r, _)| r)
    }

    /// Parse a RET_SUBMIT from the payload.
    pub fn as_ret_submit(&self) -> Option<&UsbIpRetSubmit> {
        if self.header.command.get() != super::protocol::USBIP_RET_SUBMIT {
            return None;
        }
        UsbIpRetSubmit::ref_from_prefix(&self.payload).ok().map(|(r, _)| r)
    }

    /// Parse a RET_UNLINK from the payload.
    pub fn as_ret_unlink(&self) -> Option<&UsbIpRetUnlink> {
        if self.header.command.get() != super::protocol::USBIP_RET_UNLINK {
            return None;
        }
        UsbIpRetUnlink::ref_from_prefix(&self.payload).ok().map(|(r, _)| r)
    }
}

/// A pre-allocated URB buffer for the hot path.
///
/// Reusing these avoids allocation on every transfer.
#[derive(Debug)]
pub struct UrbBuffer {
    /// Raw bytes for the wire message (header + URB struct + data).
    pub buf: Vec<u8>,
    /// Where data payload starts within `buf`.
    pub data_offset: usize,
    /// Capacity for data in this buffer.
    pub data_capacity: usize,
}

impl UrbBuffer {
    pub fn new(data_capacity: usize) -> Self {
        let header_size = UsbIpCmdSubmit::HEADER_SIZE + super::protocol::UsbIpHeader::SIZE;
        let total = header_size + data_capacity;
        Self { buf: vec![0u8; total], data_offset: header_size, data_capacity }
    }

    pub fn reset(&mut self) {
        self.buf.fill(0);
    }
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{URB_DIR_IN, URB_DIR_OUT};
    use zerocopy::IntoBytes;

    /// Construct a synthetic UsbIpCmdSubmit with caller-specified parameters.
    /// All unspecified fields use sensible defaults.
    fn build_cmd_submit(
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

    /// Construct a synthetic UsbIpRetSubmit with caller-specified parameters.
    #[allow(dead_code)]
    pub(crate) fn build_ret_submit(
        seqnum: u32,
        devid: u32,
        direction: u32,
        ep: u32,
        status: u32,
        actual_length: u32,
    ) -> UsbIpRetSubmit {
        UsbIpRetSubmit {
            seqnum: U32BE::new(seqnum),
            devid: U32BE::new(devid),
            direction: U32BE::new(direction),
            ep: U32BE::new(ep),
            status: U32BE::new(status),
            actual_length: U32BE::new(actual_length),
            start_frame: U32BE::new(0),
            number_of_packets: U32BE::new(0),
            error_count: U32BE::new(0),
            setup: [0u8; 8],
        }
    }

    #[test]
    fn test_hid_interrupt_in_roundtrip() {
        // HID IN (interrupt, endpoint 0x81, direction IN)
        let cmd = build_cmd_submit(0x81, 1, 64, [0u8; 8]);
        let bytes = cmd.as_bytes();
        let (deserialized, _rest) = UsbIpCmdSubmit::read_from_prefix(bytes).unwrap();
        assert_eq!(deserialized.ep_num(), 0x81);
        assert!(deserialized.is_in());
        assert_eq!(deserialized.data_len(), 64);
        assert!(!deserialized.is_control());
    }

    #[test]
    fn test_bulk_out_roundtrip() {
        // Bulk OUT (endpoint 0x02, direction OUT)
        let cmd = build_cmd_submit(0x02, 0, 512, [0u8; 8]);
        let bytes = cmd.as_bytes();
        let (deserialized, _rest) = UsbIpCmdSubmit::read_from_prefix(bytes).unwrap();
        assert_eq!(deserialized.ep_num(), 0x02);
        assert_eq!(deserialized.dir(), 0);
        assert_eq!(deserialized.data_len(), 512);
        assert!(!deserialized.is_in());
        assert!(!deserialized.is_control());
    }

    #[test]
    fn test_control_transfer_roundtrip() {
        // Control transfer with non-zero setup packet (GET_DESCRIPTOR request)
        let setup: [u8; 8] = [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x40, 0x00];
        let cmd = build_cmd_submit(0x00, 1, 64, setup);
        let bytes = cmd.as_bytes();
        let (deserialized, _rest) = UsbIpCmdSubmit::read_from_prefix(bytes).unwrap();
        assert!(deserialized.is_control());
        assert_eq!(deserialized.setup, setup);
        assert_eq!(deserialized.ep_num(), 0x00);
        assert_eq!(deserialized.data_len(), 64);
    }
}
