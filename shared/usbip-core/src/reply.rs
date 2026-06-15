//! USB/IP reply serialization — the single source of truth for building
//! `USBIP_RET_SUBMIT` wire-format replies.
//!
//! Every reply is: `[UsbIpHeader (8 bytes)] [UsbIpRetSubmit (44 bytes)] [data (variable)]`.
//! Before this module existed, the same struct construction and byte-extend sequence
//! was copied into four production functions and two benchmarks.  Now there is one
//! implementation, tested once, used everywhere.

use zerocopy::IntoBytes;

use crate::protocol::{UsbIpHeader, U32BE, USBIP_RET_SUBMIT};
use crate::urb::{UsbIpCmdSubmit, UsbIpRetSubmit};

/// Serialize a complete `USBIP_RET_SUBMIT` reply into a new `Vec<u8>`.
///
/// The returned buffer contains the 8-byte header, 44-byte RetSubmit, and
/// the data payload (if non-empty).  This is the allocating variant; callers
/// that already own a buffer should use [`serialize_reply_into`].
pub fn serialize_reply(
    cmd: &UsbIpCmdSubmit,
    status: i32,
    actual_length: u32,
    data: &[u8],
) -> Vec<u8> {
    let total_len = UsbIpHeader::SIZE + UsbIpRetSubmit::HEADER_SIZE + data.len();
    let mut buf = Vec::with_capacity(total_len);
    serialize_reply_into(&mut buf, cmd, status, actual_length, data);
    buf
}

/// Serialize a `USBIP_RET_SUBMIT` reply, appending into an existing buffer.
///
/// Returns the number of bytes appended.  The batcher uses this variant so
/// it can accumulate multiple replies in a single buffer before flushing.
pub fn serialize_reply_into(
    buf: &mut Vec<u8>,
    cmd: &UsbIpCmdSubmit,
    status: i32,
    actual_length: u32,
    data: &[u8],
) -> usize {
    let start = buf.len();

    let header = UsbIpHeader::new(USBIP_RET_SUBMIT);
    buf.extend_from_slice(header.as_bytes());

    let ret = UsbIpRetSubmit {
        seqnum: cmd.seqnum,
        devid: cmd.devid,
        direction: cmd.direction,
        ep: cmd.ep,
        status: U32BE::new(status as u32),
        actual_length: U32BE::new(actual_length),
        start_frame: cmd.start_frame,
        number_of_packets: cmd.number_of_packets,
        error_count: U32BE::new(if status == 0 { 0 } else { 1 }),
        setup: cmd.setup,
    };
    buf.extend_from_slice(ret.as_bytes());
    buf.extend_from_slice(data);

    buf.len() - start
}
