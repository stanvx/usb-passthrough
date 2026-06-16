//! Integration test: server wire protocol with a `FakeBackend`.
//!
//! Exercises the full USB/IP TCP wire protocol without real USB hardware.
//! Each test spawns the server handler in a background task, connects via
//! localhost, and drops the client stream to shut down cleanly.

use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use usbip_core::protocol::{
    UsbIpDeviceEntry, UsbIpHeader, OP_REP_DEVLIST, OP_REP_IMPORT, OP_REQ_DEVLIST, OP_REQ_IMPORT,
    STATUS_SUCCESS,
};
use zerocopy::{FromBytes, IntoBytes};

use usbip_server::{
    server::{handle_client, Server, ServerConfig},
    usb::UsbDeviceManager,
    usb_backend::{make_test_entry, FakeBackend},
    BandwidthLimit,
};

fn server_config() -> ServerConfig {
    ServerConfig {
        bind_address: "127.0.0.1".into(),
        port: 3240,
        allowed_vid_pid: vec![],
        require_confirmation: false,
        encryption_enabled: false,
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

// ─── OP_REQ_DEVLIST ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_devlist_returns_fake_devices() {
    let usb =
        std::sync::Arc::new(UsbDeviceManager::with_backend(Box::new(FakeBackend::new(vec![
            make_test_entry("1-1", 0x046d, 0xc260),
            make_test_entry("1-2", 0x046d, 0xc261),
        ]))));

    let (listener, addr) = bind_ephemeral().await;

    let usb_clone = usb.clone();
    let config = server_config();
    let handle = tokio::spawn(async move {
        let (stream, peer) = listener.accept().await.unwrap();
        let exports =
            std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let _ = handle_client(stream, peer, usb_clone, exports, config).await;
    });

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Send OP_REQ_DEVLIST
    let header = UsbIpHeader::new(OP_REQ_DEVLIST);
    stream.write_all(header.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();

    // Read reply header
    let mut header_buf = [0u8; 8];
    stream.read_exact(&mut header_buf).await.unwrap();

    let (rep_header, _) = UsbIpHeader::read_from_prefix(&header_buf).unwrap();
    assert_eq!(rep_header.command.get(), OP_REP_DEVLIST, "expected OP_REP_DEVLIST reply");

    // Read ndev
    let mut ndev_buf = [0u8; 4];
    stream.read_exact(&mut ndev_buf).await.unwrap();
    let ndev = u32::from_be_bytes(ndev_buf);
    assert_eq!(ndev, 2, "fake backend has 2 devices");

    // Read device entries
    for _ in 0..ndev {
        let mut entry_buf = vec![0u8; UsbIpDeviceEntry::SIZE];
        stream.read_exact(&mut entry_buf).await.unwrap();
        let (entry, _) = UsbIpDeviceEntry::read_from_prefix(&entry_buf).unwrap();
        assert!(
            entry.vid() == 0x046d && (entry.pid() == 0xc260 || entry.pid() == 0xc261),
            "expected known VID:PID",
        );
    }

    // Drop client stream so server's handle_client returns
    drop(stream);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;
}

// ─── OP_REQ_IMPORT ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_import_valid_device_returns_success() {
    let usb =
        std::sync::Arc::new(UsbDeviceManager::with_backend(Box::new(FakeBackend::new(vec![
            make_test_entry("1-1", 0x046d, 0xc260),
        ]))));

    let (listener, addr) = bind_ephemeral().await;

    let usb_clone = usb.clone();
    let config = server_config();
    let handle = tokio::spawn(async move {
        let (stream, peer) = listener.accept().await.unwrap();
        let exports =
            std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let _ = handle_client(stream, peer, usb_clone, exports, config).await;
    });

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Send OP_REQ_IMPORT for device 1-1
    let header = UsbIpHeader::new(OP_REQ_IMPORT);
    stream.write_all(header.as_bytes()).await.unwrap();

    let mut busid_buf = [0u8; 32];
    busid_buf[..3].copy_from_slice(b"1-1");
    stream.write_all(&busid_buf).await.unwrap();
    stream.flush().await.unwrap();

    // Read OP_REP_IMPORT header
    let mut header_buf = [0u8; 8];
    stream.read_exact(&mut header_buf).await.unwrap();

    let (rep_header, _) = UsbIpHeader::read_from_prefix(&header_buf).unwrap();
    assert_eq!(rep_header.command.get(), OP_REP_IMPORT);
    assert_eq!(rep_header.status.get(), STATUS_SUCCESS);

    // Read device entry (server echoes it back on success)
    let mut entry_buf = vec![0u8; UsbIpDeviceEntry::SIZE];
    stream.read_exact(&mut entry_buf).await.unwrap();
    let (entry, _) = UsbIpDeviceEntry::read_from_prefix(&entry_buf).unwrap();
    assert_eq!(entry.vid(), 0x046d);
    assert_eq!(entry.pid(), 0xc260);

    // Drop stream so server's URB loop exits
    drop(stream);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;
}

// ─── Server::with_backend constructor ─────────────────────────────────────

#[test]
fn test_with_backend_constructor_creates_server() {
    let devices = vec![make_test_entry("1-1", 0x046d, 0xc260)];
    let backend = Box::new(FakeBackend::new(devices));

    let config = server_config();
    let rt = tokio::runtime::Runtime::new().unwrap();

    let server = rt.block_on(async { Server::with_backend(config, backend).await.unwrap() });

    let devs = rt.block_on(async { server.exportable_devices().await });
    assert_eq!(devs.len(), 1);
    assert_eq!(devs[0].vid(), 0x046d);
    assert_eq!(devs[0].pid(), 0xc260);
}
