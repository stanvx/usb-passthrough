//! AES-256-GCM encryption layer over a USB/IP TCP stream.
//!
//! Wraps an established plaintext USB/IP TCP stream (after the
//! OP_REQ_DEVLIST/OP_REP_DEVLIST and OP_REQ_IMPORT/OP_REP_IMPORT
//! handshake) with an X25519 + AES-256-GCM tunnel.
//!
//! ## Wire format
//!
//! After the handshake, when `--encrypt` is enabled:
//!
//! ```text
//!   Initiator                                    Responder
//!     │  [4-byte len=32][32-byte X25519 pubkey]   │
//!     │───────────────────────────────────────────►│
//!     │                                             │
//!     │  [4-byte len=32][32-byte X25519 pubkey]    │
//!     │◄───────────────────────────────────────────│
//!     │                                             │
//!     │  HKDF-SHA256 → AES-256-GCM session key     │
//!     │                                             │
//!     │  Each message:                              │
//!     │  [4-byte ciphertext length BE]              │
//!     │  [ciphertext || 12-byte nonce || 16-byte GCM tag]
//!     │───────────────────────────────────────────►│
//! ```
//!
//! The encrypted payload is a single USB/IP message: header (8 bytes) plus
//! its variable payload (device entries, URB header, URB data, etc.). This
//! keeps the encryption layer transparent to the existing `handle_devlist`,
//! `handle_import`, and `handle_urb_loop` flows.

use std::net::SocketAddr;
use std::sync::Arc;

use ring::aead::LessSafeKey;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::info;

use usbip_core::crypto;
use usbip_core::error::{ErrorKind, UsbIpError, UsbIpResult};

/// Maximum ciphertext size for a single encrypted USB/IP message.
pub const ENCRYPTED_MAX: usize = usbip_core::MAX_MESSAGE_SIZE + 20 + 4;

/// Peer side of a USB/IP TCP connection, wrapping it in an AES-256-GCM tunnel.
pub struct CryptoStream {
    stream: TcpStream,
    key: Arc<LessSafeKey>,
    peer: SocketAddr,
}

impl CryptoStream {
    /// Wrap an existing TCP stream as the server side of the key exchange.
    pub async fn server_side(mut stream: TcpStream, peer: SocketAddr) -> UsbIpResult<Self> {
        if let Err(e) = stream.set_nodelay(true) {
            tracing::warn!("set_nodelay failed on crypto stream: {}", e);
        }

        let (server_pub, server_priv) = crypto::generate_key_pair()
            .map_err(|e| UsbIpError::from(ErrorKind::Encryption(e.to_string())))?;

        // Send our pubkey FIRST so the client (which is waiting to
        // read the server's pubkey after the OP_REP_IMPORT reply) gets
        // unblocked. Then read the client's pubkey in return. This
        // avoids a chicken-and-egg deadlock where both sides block
        // trying to read each other's pubkey before either writes.
        let mut out = Vec::with_capacity(4 + 32);
        out.extend_from_slice(&32u32.to_be_bytes());
        out.extend_from_slice(&server_pub);
        stream.write_all(&out).await?;
        stream.flush().await?;

        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let client_pub_len = u32::from_be_bytes(len_buf) as usize;
        if client_pub_len != 32 {
            return Err(UsbIpError::from(ErrorKind::Encryption(format!(
                "unexpected client pubkey length: {}",
                client_pub_len
            ))));
        }
        let mut client_pub = vec![0u8; 32];
        stream.read_exact(&mut client_pub).await?;

        let shared = crypto::agree(server_priv, &client_pub)
            .map_err(|e| UsbIpError::from(ErrorKind::Encryption(e.to_string())))?;
        let key = crypto::derive_session_key(&shared)
            .map_err(|e| UsbIpError::from(ErrorKind::Encryption(e.to_string())))?;

        info!(%peer, "AES-256-GCM tunnel established");

        Ok(Self { stream, key: Arc::new(key), peer })
    }

    /// Wrap an existing TCP stream as the client side of the key exchange.
    pub async fn client_side(mut stream: TcpStream, peer: SocketAddr) -> UsbIpResult<Self> {
        if let Err(e) = stream.set_nodelay(true) {
            tracing::warn!("set_nodelay failed on crypto stream: {}", e);
        }

        let (client_pub, client_priv) = crypto::generate_key_pair()
            .map_err(|e| UsbIpError::from(ErrorKind::Encryption(e.to_string())))?;

        let mut out = Vec::with_capacity(4 + 32);
        out.extend_from_slice(&32u32.to_be_bytes());
        out.extend_from_slice(&client_pub);
        stream.write_all(&out).await?;
        stream.flush().await?;

        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let server_pub_len = u32::from_be_bytes(len_buf) as usize;
        if server_pub_len != 32 {
            return Err(UsbIpError::from(ErrorKind::Encryption(format!(
                "unexpected server pubkey length: {}",
                server_pub_len
            ))));
        }
        let mut server_pub = vec![0u8; 32];
        stream.read_exact(&mut server_pub).await?;

        let shared = crypto::agree(client_priv, &server_pub)
            .map_err(|e| UsbIpError::from(ErrorKind::Encryption(e.to_string())))?;
        let key = crypto::derive_session_key(&shared)
            .map_err(|e| UsbIpError::from(ErrorKind::Encryption(e.to_string())))?;

        Ok(Self { stream, key: Arc::new(key), peer })
    }

    /// Read one full USB/IP message (header + payload) from the encrypted stream.
    pub async fn read_message(&mut self) -> UsbIpResult<Vec<u8>> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len == 0 || len > ENCRYPTED_MAX {
            return Err(UsbIpError::from(ErrorKind::Encryption(format!(
                "invalid encrypted frame length: {}",
                len
            ))));
        }

        let mut ct = vec![0u8; len];
        self.stream.read_exact(&mut ct).await?;

        crypto::decrypt(&self.key, &ct)
            .map_err(|e| UsbIpError::from(ErrorKind::Encryption(e.to_string())))
    }

    /// Encrypt and write one full USB/IP message (header + payload).
    pub async fn write_message(&mut self, plaintext: &[u8]) -> UsbIpResult<()> {
        let ct = crypto::encrypt_message(&self.key, plaintext)
            .map_err(|e| UsbIpError::from(ErrorKind::Encryption(e.to_string())))?;

        let mut framed = Vec::with_capacity(4 + ct.len());
        framed.extend_from_slice(&(ct.len() as u32).to_be_bytes());
        framed.extend_from_slice(&ct);

        self.stream.write_all(&framed).await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Underlying TCP stream.
    pub fn stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }

    /// Peer address this stream is bound to.
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer
    }
}

/// A USB/IP message-bearing stream that may be plaintext or AES-256-GCM encrypted.
///
/// Reads and writes whole USB/IP messages (header + payload). The plain
/// variant performs no encryption — it is a length-prefixed wrapper used
/// for code paths that should be a no-op on the wire. The encrypted
/// variant runs the X25519 + AES-256-GCM tunnel.
pub enum Wire {
    /// Plaintext with a 4-byte length prefix (matches the protocol shape
    /// so the URB loop is the same in both modes).
    Plain { stream: TcpStream, peer: SocketAddr },
    /// Encrypted tunnel.
    Encrypted(CryptoStream),
}

impl Wire {
    /// Wrap an existing TCP stream with no encryption. The URB loop
    /// reads/writes 4-byte-length-prefixed frames.
    pub fn plain(stream: TcpStream, peer: SocketAddr) -> Self {
        Self::Plain { stream, peer }
    }

    /// Wrap an existing TCP stream with encryption. Performs the X25519
    /// key exchange on `stream` before returning.
    pub async fn encrypted(stream: TcpStream, peer: SocketAddr) -> UsbIpResult<Self> {
        let crypto = CryptoStream::server_side(stream, peer).await?;
        Ok(Self::Encrypted(crypto))
    }

    /// Peer address.
    pub fn peer_addr(&self) -> SocketAddr {
        match self {
            Self::Plain { peer, .. } => *peer,
            Self::Encrypted(s) => s.peer_addr(),
        }
    }

    /// Read one USB/IP message (header + payload).
    pub async fn read_message(&mut self) -> UsbIpResult<Vec<u8>> {
        match self {
            Self::Plain { stream, .. } => {
                let mut len_buf = [0u8; 4];
                stream.read_exact(&mut len_buf).await?;
                let len = u32::from_be_bytes(len_buf) as usize;
                if len == 0 || len > ENCRYPTED_MAX {
                    return Err(UsbIpError::from(ErrorKind::InvalidMessage(format!(
                        "invalid frame length: {}",
                        len
                    ))));
                }
                let mut buf = vec![0u8; len];
                stream.read_exact(&mut buf).await?;
                Ok(buf)
            },
            Self::Encrypted(c) => c.read_message().await,
        }
    }

    /// Write one USB/IP message (header + payload).
    pub async fn write_message(&mut self, plaintext: &[u8]) -> UsbIpResult<()> {
        match self {
            Self::Plain { stream, .. } => {
                let mut framed = Vec::with_capacity(4 + plaintext.len());
                framed.extend_from_slice(&(plaintext.len() as u32).to_be_bytes());
                framed.extend_from_slice(plaintext);
                stream.write_all(&framed).await?;
                stream.flush().await?;
                Ok(())
            },
            Self::Encrypted(c) => c.write_message(plaintext).await,
        }
    }

    /// Underlying TCP stream.
    pub fn inner(&mut self) -> &mut TcpStream {
        match self {
            Self::Plain { stream, .. } => stream,
            Self::Encrypted(c) => c.stream_mut(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    async fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        (client, server)
    }

    #[tokio::test]
    async fn roundtrip_message() {
        let (client_tcp, server_tcp) = pair().await;
        let client_addr = client_tcp.local_addr().unwrap();
        let server_addr = server_tcp.local_addr().unwrap();

        // Both sides need to write their pubkey before either can read
        // the other's, so run the key exchanges concurrently.
        let client_fut = CryptoStream::client_side(client_tcp, client_addr);
        let server_fut = CryptoStream::server_side(server_tcp, server_addr);
        let (mut client, mut server) = tokio::join!(client_fut, server_fut);
        let mut client = client.unwrap();
        let mut server = server.unwrap();

        let msg = b"hello, USB/IP encrypted world";
        client.write_message(msg).await.unwrap();
        let got = server.read_message().await.unwrap();
        assert_eq!(got, msg);

        let reply = b"reply from server";
        server.write_message(reply).await.unwrap();
        let got = client.read_message().await.unwrap();
        assert_eq!(got, reply);
    }

    #[tokio::test]
    async fn pubkey_length_prefix_is_32() {
        let (mut client_tcp, server_tcp) = pair().await;
        let server_addr = server_tcp.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let _ = CryptoStream::server_side(server_tcp, server_addr).await;
        });

        let mut prefix = [0u8; 4];
        client_tcp.read_exact(&mut prefix).await.unwrap();
        let len = u32::from_be_bytes(prefix) as usize;
        assert_eq!(len, 32, "server pubkey length prefix must be 32");

        let mut pubkey = vec![0u8; len];
        client_tcp.read_exact(&mut pubkey).await.unwrap();

        // Send our own pubkey to unblock the server's read.
        let mut client_pub = vec![0u8; 32];
        client_tcp.write_all(&(32u32.to_be_bytes())).await.unwrap();
        client_tcp.write_all(&client_pub).await.unwrap();
        client_tcp.flush().await.unwrap();

        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), server_task).await;
    }
}
