//! Benchmarks for URB forwarding — comparing copy-based vs zero-copy paths.
//!
//! Run with: cargo bench -p usbip-server

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use usbip_core::urb::{UsbIpCmdSubmit, UsbIpRetSubmit, U32BE};
use usbip_core::protocol::{UsbIpHeader, URB_DIR_IN, USBIP_RET_SUBMIT};
use zerocopy::IntoBytes;

/// Simulate the current copy-based reply building.
fn build_reply_copy(cmd: &UsbIpCmdSubmit, data: &[u8], status: i32) -> Vec<u8> {
    let ret = UsbIpRetSubmit {
        seqnum: cmd.seqnum,
        devid: cmd.devid,
        direction: cmd.direction,
        ep: cmd.ep,
        status: U32BE::new(status as u32),
        actual_length: U32BE::new(data.len() as u32),
        start_frame: cmd.start_frame,
        number_of_packets: cmd.number_of_packets,
        error_count: U32BE::new(if status == 0 { 0 } else { 1 }),
        setup: cmd.setup,
    };

    let mut reply = Vec::new();
    let ret_header = UsbIpHeader::new(USBIP_RET_SUBMIT);
    reply.extend_from_slice(ret_header.as_bytes());
    reply.extend_from_slice(ret.as_bytes());
    if !data.is_empty() {
        reply.extend_from_slice(data);
    }
    reply
}

/// Zero-copy variant: writes directly into a pre-allocated buffer.
fn build_reply_zero_copy<'a>(
    buf: &'a mut [u8],
    cmd: &UsbIpCmdSubmit,
    data: &[u8],
    status: i32,
) -> &'a mut [u8] {
    let header_offset = 0;
    let ret_offset = UsbIpHeader::SIZE;
    let data_offset = ret_offset + UsbIpRetSubmit::HEADER_SIZE;
    let total_len = data_offset + data.len();

    // Write UsbIpHeader
    let hdr = UsbIpHeader::new(USBIP_RET_SUBMIT);
    let hdr_bytes = hdr.as_bytes();
    buf[header_offset..header_offset + hdr_bytes.len()].copy_from_slice(hdr_bytes);

    // Write UsbIpRetSubmit
    let ret = UsbIpRetSubmit {
        seqnum: cmd.seqnum,
        devid: cmd.devid,
        direction: cmd.direction,
        ep: cmd.ep,
        status: U32BE::new(status as u32),
        actual_length: U32BE::new(data.len() as u32),
        start_frame: cmd.start_frame,
        number_of_packets: cmd.number_of_packets,
        error_count: U32BE::new(if status == 0 { 0 } else { 1 }),
        setup: cmd.setup,
    };
    let ret_bytes = ret.as_bytes();
    buf[ret_offset..ret_offset + ret_bytes.len()].copy_from_slice(ret_bytes);

    // Write data (the "zero copy" — we're writing into a pre-allocated buffer)
    if !data.is_empty() {
        buf[data_offset..total_len].copy_from_slice(data);
    }

    &mut buf[..total_len]
}

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

fn bench_forward_copy_small(c: &mut Criterion) {
    let cmd = make_cmd(1, 0x81, 64);
    let data = vec![0xABu8; 64];

    c.bench_function("forward/copy_small_64b", |b| {
        b.iter(|| {
            let reply = build_reply_copy(&cmd, &data, 0);
            black_box(reply.len());
        })
    });
}

fn bench_forward_copy_large(c: &mut Criterion) {
    let cmd = make_cmd(1, 0x02, 16384);
    let data = vec![0xABu8; 16384];

    c.bench_function("forward/copy_large_16k", |b| {
        b.iter(|| {
            let reply = build_reply_copy(&cmd, &data, 0);
            black_box(reply.len());
        })
    });
}

fn bench_forward_zero_copy_small(c: &mut Criterion) {
    let cmd = make_cmd(1, 0x81, 64);
    let data = vec![0xABu8; 64];
    let total_size = UsbIpHeader::SIZE + UsbIpRetSubmit::HEADER_SIZE + data.len();
    let mut buf = vec![0u8; total_size];

    c.bench_function("forward/zero_copy_small_64b", |b| {
        b.iter(|| {
            let reply = build_reply_zero_copy(&mut buf, &cmd, &data, 0);
            black_box(reply.len());
        })
    });
}

fn bench_forward_zero_copy_large(c: &mut Criterion) {
    let cmd = make_cmd(1, 0x02, 16384);
    let data = vec![0xABu8; 16384];
    let total_size = UsbIpHeader::SIZE + UsbIpRetSubmit::HEADER_SIZE + data.len();
    let mut buf = vec![0u8; total_size];

    c.bench_function("forward/zero_copy_large_16k", |b| {
        b.iter(|| {
            let reply = build_reply_zero_copy(&mut buf, &cmd, &data, 0);
            black_box(reply.len());
        })
    });
}

fn bench_forward_zero_copy_reuse(c: &mut Criterion) {
    let cmd = make_cmd(1, 0x81, 64);
    let data = vec![0xABu8; 64];
    let total_size = UsbIpHeader::SIZE + UsbIpRetSubmit::HEADER_SIZE + data.len();
    let mut buf = vec![0u8; total_size];

    c.bench_function("forward/zero_copy_reuse_buffer", |b| {
        b.iter(|| {
            // Reuse the same buffer for multiple forwards
            for i in 0..100 {
                let cmd2 = make_cmd(i, 0x81, 64);
                let reply = build_reply_zero_copy(&mut buf, &cmd2, &data, 0);
                black_box(reply.len());
            }
        })
    });
}

fn bench_forward_copy_reuse(c: &mut Criterion) {
    let cmd = make_cmd(1, 0x81, 64);
    let data = vec![0xABu8; 64];

    c.bench_function("forward/copy_reuse_alloc", |b| {
        b.iter(|| {
            for i in 0..100 {
                let cmd2 = make_cmd(i, 0x81, 64);
                let reply = build_reply_copy(&cmd2, &data, 0);
                black_box(reply.len());
            }
        })
    });
}

criterion_group! {
    name = forward_benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(3))
        .warm_up_time(Duration::from_secs(1))
        .sample_size(50);
    targets =
        bench_forward_copy_small,
        bench_forward_copy_large,
        bench_forward_zero_copy_small,
        bench_forward_zero_copy_large,
        bench_forward_zero_copy_reuse,
        bench_forward_copy_reuse,
}

criterion_main!(forward_benches);
