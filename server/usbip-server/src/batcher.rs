//! URB batch submission — aggregate sequential URB replies before flushing.
//!
//! Instead of writing each URB reply to the TCP socket individually, the
//! batcher collects them into a single buffer and flushes on: timer expiry
//! (200 µs max delay), buffer full, or a non-sequential command.
//!
//! This reduces the number of TCP segments sent, improving throughput for
//! high-frequency interrupt endpoints (e.g., HID polling at 1000 Hz).

use std::time::{Duration, Instant};

use tracing::trace;
use zerocopy::IntoBytes;

use usbip_core::protocol::{UsbIpHeader, USBIP_RET_SUBMIT};
use usbip_core::urb::UsbIpCmdSubmit;

use crate::urb_executor::UrbResult;

/// Default maximum number of URBs per batch.
pub const DEFAULT_BATCH_SIZE: usize = 8;

/// Default maximum delay before flushing (200 µs).
pub const DEFAULT_BATCH_TIMEOUT: Duration = Duration::from_micros(200);

/// Maximum buffer capacity for a single batch (64 KiB).
const MAX_BATCH_CAPACITY: usize = 64 * 1024;

/// Accumulates sequential URB replies for batched TCP writes.
pub struct UrbBatcher {
    /// Serialised reply bytes accumulated so far.
    buffer: Vec<u8>,
    /// Maximum number of URBs per batch.
    max_batch: usize,
    /// Maximum delay before auto-flush.
    timeout: Duration,
    /// Instant when the first URB of this batch was added.
    batch_start: Option<Instant>,
    /// Last `seqnum` seen — used to detect non-sequential commands that force a flush.
    last_seqnum: Option<u32>,
    /// Number of URBs in the current batch.
    count: usize,
}

impl UrbBatcher {
    /// Create a new batcher with default settings.
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(MAX_BATCH_CAPACITY.min(4096)),
            max_batch: DEFAULT_BATCH_SIZE,
            timeout: DEFAULT_BATCH_TIMEOUT,
            batch_start: None,
            last_seqnum: None,
            count: 0,
        }
    }

    /// Create a batcher with custom max batch size and timeout.
    pub fn with_config(max_batch: usize, timeout: Duration) -> Self {
        Self {
            buffer: Vec::with_capacity(MAX_BATCH_CAPACITY.min(4096)),
            max_batch,
            timeout,
            batch_start: None,
            last_seqnum: None,
            count: 0,
        }
    }

    /// Add a URB reply to the batch and return whether the batch should be flushed.
    ///
    /// Returns `true` if the caller MUST flush the batch before processing
    /// further URBs (batch is full, non-sequential seqnum, or timer expired).
    pub fn push(&mut self, cmd: &UsbIpCmdSubmit, result: &UrbResult) -> bool {
        let seqnum = cmd.seqnum();

        // Force flush on non-sequential seqnum.
        if let Some(last) = self.last_seqnum {
            if seqnum != last.wrapping_add(1) && !self.buffer.is_empty() {
                trace!("non-sequential seqnum: last={}, current={} — flushing batch", last, seqnum);
                // Don't insert the current URB yet — caller must flush first.
                return true;
            }
        }

        // Force flush if batch is full.
        if self.count >= self.max_batch && !self.buffer.is_empty() {
            return true;
        }

        // Force flush if timer expired.
        if let Some(start) = self.batch_start {
            if start.elapsed() >= self.timeout && !self.buffer.is_empty() {
                return true;
            }
        }

        // Build the reply bytes.
        let ret_header = UsbIpHeader::new(USBIP_RET_SUBMIT);
        let ret_submit = usbip_core::urb::UsbIpRetSubmit {
            seqnum: cmd.seqnum,
            devid: cmd.devid,
            direction: cmd.direction,
            ep: cmd.ep,
            status: usbip_core::protocol::U32BE::new(result.status as u32),
            actual_length: usbip_core::protocol::U32BE::new(result.actual_length),
            start_frame: cmd.start_frame,
            number_of_packets: cmd.number_of_packets,
            error_count: usbip_core::protocol::U32BE::new(if result.status == 0 { 0 } else { 1 }),
            setup: cmd.setup,
        };

        // Check if adding this would exceed max buffer capacity.
        let reply_size = 8 + 40 + result.data.len();
        if self.buffer.len() + reply_size > MAX_BATCH_CAPACITY {
            return true;
        }

        // Add to batch.
        self.buffer.extend_from_slice(ret_header.as_bytes());
        self.buffer.extend_from_slice(ret_submit.as_bytes());
        if !result.data.is_empty() {
            self.buffer.extend_from_slice(&result.data);
        }

        self.count += 1;
        self.last_seqnum = Some(seqnum);

        if self.batch_start.is_none() {
            self.batch_start = Some(Instant::now());
        }

        // Return true if the batch is now full.
        self.count >= self.max_batch
    }

    /// Check if the batch has timed out and should be flushed.
    ///
    /// This is a non-blocking check — no I/O, just a time comparison.
    pub fn should_timeout(&self) -> bool {
        self.batch_start.map_or(false, |start| start.elapsed() >= self.timeout)
            && !self.buffer.is_empty()
    }

    /// Consume the batch and return the accumulated bytes for writing.
    ///
    /// Returns an empty `Vec` if there is nothing to flush.  The batcher
    /// is reset and ready for the next batch.
    pub fn flush(&mut self) -> Vec<u8> {
        let data = std::mem::take(&mut self.buffer);
        self.buffer = Vec::with_capacity(MAX_BATCH_CAPACITY.min(4096));
        self.batch_start = None;
        self.count = 0;
        // last_seqnum is intentionally NOT reset — it stays across batches
        // so non-sequential detection still works.
        data
    }

    /// Number of URBs currently in the batch.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use usbip_core::protocol::U32BE;
    use usbip_core::protocol::URB_DIR_IN;

    fn make_cmd(seqnum: u32, ep: u32, len: u32) -> UsbIpCmdSubmit {
        UsbIpCmdSubmit {
            seqnum: U32BE::new(seqnum),
            devid: U32BE::new(1),
            direction: U32BE::new(1),
            ep: U32BE::new(ep),
            transfer_flags: U32BE::new(URB_DIR_IN),
            transfer_buffer_length: U32BE::new(len),
            start_frame: U32BE::new(0),
            number_of_packets: U32BE::new(0),
            interval: U32BE::new(0),
            setup: [0u8; 8],
        }
    }

    fn make_result(len: u32) -> UrbResult {
        UrbResult { status: 0, actual_length: len, data: vec![0xAB; len as usize] }
    }

    // ── Basic batching ──────────────────────────────────────────

    #[test]
    fn test_new_batcher_is_empty() {
        let b = UrbBatcher::new();
        assert!(b.is_empty());
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn test_push_first_urb_does_not_flush() {
        let mut b = UrbBatcher::new();
        let cmd = make_cmd(1, 0x81, 8);
        let result = make_result(8);
        assert!(!b.push(&cmd, &result), "first URB should not trigger flush");
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn test_push_up_to_batch_size() {
        let mut b = UrbBatcher::with_config(4, Duration::from_secs(60));
        for i in 1..=4 {
            let cmd = make_cmd(i, 0x81, 4);
            let result = make_result(4);
            let should_flush = b.push(&cmd, &result);
            if i < 4 {
                assert!(!should_flush, "URB {} should not flush", i);
            } else {
                assert!(should_flush, "URB 4 (batch full) should flush");
            }
        }
        assert_eq!(b.len(), 4);
    }

    #[test]
    fn test_flush_returns_data_and_resets() {
        let mut b = UrbBatcher::new();
        let cmd = make_cmd(1, 0x81, 8);
        let result = make_result(8);
        b.push(&cmd, &result);

        let data = b.flush();
        assert!(!data.is_empty(), "flush should return data");
        assert!(b.is_empty(), "batcher should be empty after flush");
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn test_flush_empty_returns_empty() {
        let mut b = UrbBatcher::new();
        let data = b.flush();
        assert!(data.is_empty(), "flush on empty batcher should return empty vec");
    }

    // ── Non-sequential seqnum ──────────────────────────────────

    #[test]
    fn test_push_non_sequential_triggers_flush() {
        let mut b = UrbBatcher::new();
        let cmd1 = make_cmd(1, 0x81, 4);
        let result1 = make_result(4);
        b.push(&cmd1, &result1);

        // Non-sequential (jump from 1 to 5).
        let cmd5 = make_cmd(5, 0x81, 4);
        let result5 = make_result(4);
        let should_flush = b.push(&cmd5, &result5);
        assert!(should_flush, "non-sequential seqnum should trigger flush");
    }

    #[test]
    fn test_sequential_seqnus_stay_in_batch() {
        let mut b = UrbBatcher::new();
        for i in 1..=5 {
            let cmd = make_cmd(i, 0x81, 4);
            let result = make_result(4);
            let _ = b.push(&cmd, &result);
        }
        // All sequential, so they all fit until batch fills.
        assert!(!b.is_empty());
    }

    #[test]
    fn test_seqnum_wraparound_is_sequential() {
        let mut b = UrbBatcher::new();
        let cmd_max = make_cmd(u32::MAX, 0x81, 4);
        let result_max = make_result(4);
        b.push(&cmd_max, &result_max);

        // u32::MAX + 1 should wrap to 0 — that's still sequential.
        let cmd_wrap = make_cmd(0, 0x81, 4);
        let result_wrap = make_result(4);
        let should_flush = b.push(&cmd_wrap, &result_wrap);
        assert!(!should_flush, "u32 wraparound should be treated as sequential");
    }

    // ── Buffer capacity ────────────────────────────────────────

    #[test]
    fn test_large_urb_triggers_capacity_flush() {
        let mut b = UrbBatcher::new();
        // Create a URB reply that would exceed MAX_BATCH_CAPACITY.
        let cmd = make_cmd(1, 0x81, 65536);
        let result = UrbResult { status: 0, actual_length: 65536, data: vec![0; 65536] };
        let should_flush = b.push(&cmd, &result);
        // Should flush because adding it would exceed MAX_BATCH_CAPACITY.
        assert!(should_flush);
    }

    #[test]
    fn test_timeout_check() {
        let mut b = UrbBatcher::new();
        assert!(!b.should_timeout(), "empty batcher should not timeout");

        let cmd = make_cmd(1, 0x81, 4);
        let result = make_result(4);
        b.push(&cmd, &result);

        // Timeout should not trigger immediately (only 200 µs).
        assert!(!b.should_timeout(), "timeout should not trigger immediately");
    }

    // ── Edge cases ─────────────────────────────────────────────

    #[test]
    fn test_push_after_flush_resumes_batching() {
        let mut b = UrbBatcher::new();
        let cmd1 = make_cmd(1, 0x81, 4);
        let result1 = make_result(4);
        b.push(&cmd1, &result1);
        let _ = b.flush();

        assert!(b.is_empty());
        assert_eq!(b.len(), 0);

        // Push after flush should work normally.
        let cmd2 = make_cmd(2, 0x81, 4);
        let result2 = make_result(4);
        b.push(&cmd2, &result2);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn test_mixed_in_out_urbs() {
        let mut b = UrbBatcher::new();
        // IN URB
        let cmd_in = make_cmd(1, 0x81, 8);
        let result_in = make_result(8);
        b.push(&cmd_in, &result_in);

        // OUT URB (no data in reply)
        let cmd_out = UsbIpCmdSubmit {
            seqnum: U32BE::new(2),
            devid: U32BE::new(1),
            direction: U32BE::new(0),
            ep: U32BE::new(0x02),
            transfer_flags: U32BE::new(0), // OUT
            transfer_buffer_length: U32BE::new(0),
            start_frame: U32BE::new(0),
            number_of_packets: U32BE::new(0),
            interval: U32BE::new(0),
            setup: [0u8; 8],
        };
        let result_out = UrbResult { status: 0, actual_length: 4, data: Vec::new() };
        let should_flush = b.push(&cmd_out, &result_out);
        assert!(!should_flush, "mix of IN/OUT should batch normally");
        assert_eq!(b.len(), 2);
    }
}
