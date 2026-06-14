//! Encryption for USB/IP messages — AES-256-GCM with X25519 key exchange.
//!
//! Optional layer. When enabled, all USB/IP messages after the TCP handshake
//! are encrypted per-message with unique 96-bit nonces.
//!
//! ## Protocol
//!
//! Client                                    Server
//!   │  X25519 ephemeral public key           │
//!   │───────────────────────────────────────►│
//!   │                                         │
//!   │         X25519 ephemeral public key     │
//!   │◄───────────────────────────────────────│
//!   │                                         │
//!   │  HKDF-SHA256 → AES-256-GCM session key  │
//!   │                                         │
//!   │  Each message:                          │
//!   │  [4-byte nonce][ciphertext+tag]         │
//!   │───────────────────────────────────────►│

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::agreement::{agree_ephemeral, EphemeralPrivateKey, UnparsedPublicKey, X25519};
use ring::hkdf::{KeyType, Salt, HKDF_SHA256};
use ring::rand::{SecureRandom, SystemRandom};

pub type CryptoResult<T> = Result<T, CryptoError>;

#[derive(Debug)]
pub enum CryptoError {
    KeyGeneration,
    KeyAgreement,
    KeyDerivation,
    Encryption,
    Decryption,
    InvalidNonce,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KeyGeneration => write!(f, "key generation failed"),
            Self::KeyAgreement => write!(f, "key agreement failed"),
            Self::KeyDerivation => write!(f, "key derivation failed"),
            Self::Encryption => write!(f, "encryption failed"),
            Self::Decryption => write!(f, "decryption failed"),
            Self::InvalidNonce => write!(f, "invalid nonce"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// Maximum ciphertext overhead: 4-byte nonce prefix + AES-GCM 16-byte tag.
pub const ENCRYPTION_OVERHEAD: usize = 20;

// ─── Key Generation ──────────────────────────────────────────────

/// Generate an X25519 ephemeral key pair.
///
/// Returns (public_key_bytes, private_key_handle).
/// The private key is consumed by [`agree()`] and cannot be serialized.
pub fn generate_key_pair() -> CryptoResult<(Vec<u8>, EphemeralPrivateKey)> {
    let rng = SystemRandom::new();
    let private_key =
        EphemeralPrivateKey::generate(&X25519, &rng).map_err(|_| CryptoError::KeyGeneration)?;
    let public_key = private_key.compute_public_key().map_err(|_| CryptoError::KeyGeneration)?;
    Ok((public_key.as_ref().to_vec(), private_key))
}

/// Generate key pair, returned as hex public key + private key handle (for JNI bridge).
pub fn generate_key_pair_hex() -> CryptoResult<(String, EphemeralPrivateKey)> {
    let (pub_bytes, priv_key) = generate_key_pair()?;
    Ok((hex_encode(&pub_bytes), priv_key))
}

// ─── Key Agreement ───────────────────────────────────────────────

/// Perform X25519 ECDH key agreement.
///
/// Consumes the ephemeral private key (ring 0.17 design).
/// Returns the 32-byte shared secret.
pub fn agree(
    private_key: EphemeralPrivateKey,
    peer_public_key_bytes: &[u8],
) -> CryptoResult<Vec<u8>> {
    let peer_public_key = UnparsedPublicKey::new(&X25519, peer_public_key_bytes);
    agree_ephemeral(private_key, &peer_public_key, |key_material| key_material.to_vec())
        .map_err(|_| CryptoError::KeyAgreement)
}

/// Derive an AES-256-GCM session key via HKDF-SHA256.
///
/// Requires the 32-byte shared secret from ECDH.
pub fn derive_session_key(shared_secret: &[u8]) -> CryptoResult<LessSafeKey> {
    let salt = Salt::new(HKDF_SHA256, b"USBIP-PASSTHROUGH-V1");
    let info = b"usbip-session-key";
    let prk = salt.extract(shared_secret);
    let info_slices: &[&[u8]] = &[info];
    let okm = prk.expand(info_slices, Aes256GcmKeyType).map_err(|_| CryptoError::KeyDerivation)?;

    let mut key_bytes = [0u8; 32];
    okm.fill(&mut key_bytes).map_err(|_| CryptoError::KeyDerivation)?;

    let unbound =
        UnboundKey::new(&AES_256_GCM, &key_bytes).map_err(|_| CryptoError::KeyDerivation)?;
    Ok(LessSafeKey::new(unbound))
}

/// Derive session key from EphemeralPrivateKey + hex-encoded peer public key.
pub fn derive_session_key_hex(
    private_key: EphemeralPrivateKey,
    peer_public_key_hex: &str,
) -> CryptoResult<Vec<u8>> {
    let pub_bytes = hex_decode(peer_public_key_hex).ok_or(CryptoError::KeyDerivation)?;
    let shared_secret = agree(private_key, &pub_bytes)?;
    let mut key_bytes = [0u8; 32];
    derive_session_key_to_bytes(&shared_secret, &mut key_bytes)?;
    Ok(key_bytes.to_vec())
}

fn derive_session_key_to_bytes(shared_secret: &[u8], out: &mut [u8; 32]) -> CryptoResult<()> {
    let salt = Salt::new(HKDF_SHA256, b"USBIP-PASSTHROUGH-V1");
    let info = b"usbip-session-key";
    let prk = salt.extract(shared_secret);
    let info_slices: &[&[u8]] = &[info];
    let okm = prk.expand(info_slices, Aes256GcmKeyType).map_err(|_| CryptoError::KeyDerivation)?;
    okm.fill(out.as_mut_slice()).map_err(|_| CryptoError::KeyDerivation)
}

// ─── Encrypt / Decrypt ───────────────────────────────────────

/// Encrypt plaintext with AES-256-GCM using a per-message random nonce.
///
/// Wire format: `[4-byte nonce_len (=12)][12-byte nonce][ciphertext || 16-byte GCM tag]`
/// — same as `encrypt_message`, just with the nonce generated here.
pub fn encrypt(key: &LessSafeKey, plaintext: &[u8]) -> CryptoResult<Vec<u8>> {
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes).map_err(|_| CryptoError::Encryption)?;

    encrypt_with_nonce(key, plaintext, &nonce_bytes)
}

/// Encrypt with raw key bytes (for JNI bridge).
pub fn encrypt_with_key_bytes(key_bytes: &[u8], plaintext: &[u8]) -> CryptoResult<Vec<u8>> {
    let unbound = UnboundKey::new(&AES_256_GCM, key_bytes).map_err(|_| CryptoError::Encryption)?;
    let key = LessSafeKey::new(unbound);
    encrypt(&key, plaintext)
}

/// Decrypt ciphertext produced by `encrypt_message()` / `encrypt_with_nonce()`.
///
/// Wire format: `[4-byte nonce_len (=12)][12-byte nonce][ciphertext || 16-byte GCM tag]`
pub fn decrypt(key: &LessSafeKey, wire_data: &[u8]) -> CryptoResult<Vec<u8>> {
    if wire_data.len() < 4 + 12 + 16 {
        // minimum: 4-byte len + 12-byte nonce + 16-byte tag
        return Err(CryptoError::Decryption);
    }

    let nonce_len =
        u32::from_be_bytes([wire_data[0], wire_data[1], wire_data[2], wire_data[3]]) as usize;
    if nonce_len != 12 || wire_data.len() < 4 + 12 + 16 {
        return Err(CryptoError::InvalidNonce);
    }

    let nonce_bytes: [u8; 12] =
        wire_data[4..16].try_into().map_err(|_| CryptoError::InvalidNonce)?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let aad = Aad::empty();

    let mut buf = wire_data[16..].to_vec();
    let plaintext = key.open_in_place(nonce, aad, &mut buf).map_err(|_| CryptoError::Decryption)?;
    // `open_in_place` returns a slice that already excludes the GCM tag
    // and uses the trailing 16 bytes of `buf` as scratch (zeroed on
    // success). Use the returned slice, not `buf`, so we don't ship
    // the scratch zeros back to the caller.
    Ok(plaintext.to_vec())
}

/// Decrypt with raw key bytes (for JNI bridge).
pub fn decrypt_with_key_bytes(key_bytes: &[u8], wire_data: &[u8]) -> CryptoResult<Vec<u8>> {
    let unbound = UnboundKey::new(&AES_256_GCM, key_bytes).map_err(|_| CryptoError::Decryption)?;
    let key = LessSafeKey::new(unbound);
    decrypt(&key, wire_data)
}

// ─── Improved encrypt with explicit nonce ─────────────────────

/// Encrypt plaintext. Returns [4-byte nonce_len][12-byte nonce][ciphertext+tag].
pub fn encrypt_message(key: &LessSafeKey, plaintext: &[u8]) -> CryptoResult<Vec<u8>> {
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes).map_err(|_| CryptoError::Encryption)?;

    encrypt_with_nonce(key, plaintext, &nonce_bytes)
}

/// Encrypt with explicit nonce.
///
/// `seal_in_place_append_tag` encrypts in place and **appends** the GCM tag
/// to the buffer (it does not write into pre-allocated space). So the
/// output is `plaintext.len() + TAG_LEN` bytes: ciphertext then tag.
pub fn encrypt_with_nonce(
    key: &LessSafeKey,
    plaintext: &[u8],
    nonce_bytes: &[u8; 12],
) -> CryptoResult<Vec<u8>> {
    let nonce = Nonce::assume_unique_for_key(*nonce_bytes);
    let aad = Aad::empty();

    let mut buf = plaintext.to_vec();
    key.seal_in_place_append_tag(nonce, aad, &mut buf).map_err(|_| CryptoError::Encryption)?;

    // Wire format: [4-byte nonce_len = 12][12-byte nonce][ciphertext+tag]
    let mut result = Vec::with_capacity(4 + 12 + buf.len());
    result.extend_from_slice(&12u32.to_be_bytes());
    result.extend_from_slice(nonce_bytes);
    result.extend_from_slice(&buf);

    Ok(result)
}

// ─── Helpers ──────────────────────────────────────────────────

struct Aes256GcmKeyType;
impl KeyType for Aes256GcmKeyType {
    fn len(&self) -> usize {
        32
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_exchange_roundtrip() {
        let (alice_pub, alice_priv) = generate_key_pair().unwrap();
        let (bob_pub, bob_priv) = generate_key_pair().unwrap();

        let alice_secret = agree(alice_priv, &bob_pub).unwrap();
        let bob_secret = agree(bob_priv, &alice_pub).unwrap();

        assert_eq!(alice_secret, bob_secret);
    }

    #[test]
    fn test_encrypt_decrypt() {
        let (_, priv_bytes) = generate_key_pair().unwrap();
        let (_, _peer_priv) = generate_key_pair().unwrap();
        let (peer_pub, _) = generate_key_pair().unwrap();

        // Use a fixed shared secret for test determinism
        let shared = agree(priv_bytes, &peer_pub).unwrap();
        let key = derive_session_key(&shared).unwrap();

        let plaintext = b"Hello, USB/IP!";
        let encrypted = encrypt_message(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), &decrypted);
    }

    #[test]
    fn test_hex_roundtrip() {
        let original = vec![0xde, 0xad, 0xbe, 0xef];
        let hex = hex_encode(&original);
        let decoded = hex_decode(&hex).unwrap();
        assert_eq!(original, decoded);
    }
}
