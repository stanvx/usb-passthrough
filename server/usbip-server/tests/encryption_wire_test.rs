//! Integration test: --encrypt wraps the post-handshake stream in AES-256-GCM.
//!
//! Audit verification (§2.6): when --encrypt is enabled, raw bytes flowing
//! through the socket after the OP_REQ_DEVLIST/OP_REP_DEVLIST and
//! OP_REQ_IMPORT/OP_REP_IMPORT handshakes must NOT be recognizable as
//! USB/IP frames. They are the server's X25519 public key, length-prefixed
//! with 4 bytes BE = 32. The first 4 bytes should be [0x00, 0x00, 0x00, 0x20]
//! (length 32) — never 0x00000111 (USB/IP version).

use std::net::SocketAddr;

use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use zerocopy::{FromBytes, IntoBytes};

use usbip_core::protocol::{
    UsbIpDeviceEntry, UsbIpHeader, OP_REP_IMPORT, OP_REQ_IMPORT, STATUS_SUCCESS,
};
use usbip_server::{
    server::{handle_client, ServerConfig},
    usb::UsbDeviceManager,
    usb_backend::{make_test_entry, FakeBackend},
    BandwidthLimit,
};

fn server_config_encrypted() -> ServerConfig {
    ServerConfig {
        bind_address: "127.0.0.1".into(),
        port: 3240,
        allowed_vid_pid: vec![],
        require_confirmation: false,
        encryption_enabled: true,
        tcp_nodelay: true,
        max_bandwidth: BandwidthLimit::unlimited(),
        per_client_bandwidth: None,
    }
}

async fn bind_ephemeral() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    (listener, addr)
}

#[tokio::test]
async fn test_encrypted_stream_post_handshake_bytes_not_plaintext() {
    let busid = "1-1";
    let usb =
        std::sync::Arc::new(UsbDeviceManager::with_backend(Box::new(FakeBackend::new(vec![
            make_test_entry(busid, 0x046d, 0xc260),
        ]))));

    let (listener, addr) = bind_ephemeral().await;

    let usb_clone = usb.clone();
    let config = server_config_encrypted();
    let server_task = tokio::spawn(async move {
        let (stream, peer) = listener.accept().await.unwrap();
        let exports =
            std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let _ = handle_client(stream, peer, usb_clone, exports, config).await;
    });

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Send plaintext OP_REQ_IMPORT for the device.
    let header = UsbIpHeader::new(OP_REQ_IMPORT);
    stream.write_all(header.as_bytes()).await.unwrap();
    let mut busid_buf = [0u8; 32];
    busid_buf[..busid.len()].copy_from_slice(busid.as_bytes());
    stream.write_all(&busid_buf).await.unwrap();
    stream.flush().await.unwrap();

    // Read plaintext OP_REP_IMPORT (header + device entry + descriptors).
    let mut header_buf = [0u8; 8];
    stream.read_exact(&mut header_buf).await.unwrap();
    let (rep_header, _) = UsbIpHeader::read_from_prefix(&header_buf).unwrap();
    assert_eq!(rep_header.command.get(), OP_REP_IMPORT);
    assert_eq!(rep_header.status.get(), STATUS_SUCCESS);

    let mut entry_buf = vec![0u8; UsbIpDeviceEntry::SIZE];
    stream.read_exact(&mut entry_buf).await.unwrap();

    // The FakeBackend returns 4 descriptor bytes (a stub device
    // descriptor). Drain them so the next read is the start of the
    // encryption layer.
    let mut desc = [0u8; 4];
    stream.read_exact(&mut desc).await.unwrap();

    // The audit's key check: with --encrypt enabled, the very next
    // bytes on the wire (after OP_REP_IMPORT) MUST NOT parse as a
    // USB/IP header. They are the server's X25519 public key,
    // length-prefixed with 4 bytes BE = 32.
    let mut next_bytes = [0u8; 8];
    stream.read_exact(&mut next_bytes).await.unwrap();

    let looks_like_usb_header = (|| -> bool {
        let (h, _) = match UsbIpHeader::read_from_prefix(&next_bytes) {
            Ok(pair) => pair,
            Err(_) => return false,
        };
        h.version.get() == 0x0111
    })();
    assert!(
        !looks_like_usb_header,
        "with --encrypt, post-OP_REP_IMPORT bytes must not parse as USB/IP (got {:02x?})",
        next_bytes,
    );

    // The first 4 bytes are the server's X25519 pubkey length (32 BE).
    let first_len =
        u32::from_be_bytes([next_bytes[0], next_bytes[1], next_bytes[2], next_bytes[3]]);
    assert_eq!(
        first_len, 32,
        "expected 4-byte length prefix of 32 for the server's X25519 pubkey (got {})",
        first_len,
    );

    drop(stream);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), server_task).await;
}
