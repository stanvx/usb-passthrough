//! USB Request Block (URB) types — the core data-transfer primitives.

use zerocopy::byteorder::{BigEndian, U32};
use zerocopy::{AsBytes, FromBytes, FromZeroes};

pub type U32BE = U32<BigEndian>;

/// USBIP_CMD_SUBMIT — Client requests a USB transfer.
///
/// Wire format (80 bytes + variable data for OUT transfers):
#[derive(Debug, Clone, AsBytes, FromBytes, FromZeroes)]
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
#[derive(Debug, Clone, AsBytes, FromBytes, FromZeroes)]
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
    pub const HEADER_SIZE: usize = 40;

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
#[derive(Debug, Clone, AsBytes, FromBytes, FromZeroes)]
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
        UsbIpCmdSubmit::ref_from_prefix(&self.payload)
    }

    /// Parse a RET_SUBMIT from the payload.
    pub fn as_ret_submit(&self) -> Option<&UsbIpRetSubmit> {
        if self.header.command.get() != super::protocol::USBIP_RET_SUBMIT {
            return None;
        }
        UsbIpRetSubmit::ref_from_prefix(&self.payload)
    }

    /// Parse a RET_UNLINK from the payload.
    pub fn as_ret_unlink(&self) -> Option<&UsbIpRetUnlink> {
        if self.header.command.get() != super::protocol::USBIP_RET_UNLINK {
            return None;
        }
        UsbIpRetUnlink::ref_from_prefix(&self.payload)
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
