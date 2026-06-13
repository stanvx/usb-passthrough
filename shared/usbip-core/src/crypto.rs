//! Cryptographic operations for USB/IP secure passthrough.
//!
//! Provides X25519 ECDH key exchange, AES-256-GCM encrypt/decrypt,
//! and HKDF-SHA256 key derivation — all via the `ring` crate.
//!
//! # Protocol Flow
//!
//! 1. **Key exchange:** Each side generates an X25519 key pair.
//!    Public keys are exchanged out of band (e.g., inside the
//!    USB/IP control channel).
//! 2. **Session key derivation:** Each side computes
//!    `ECDH(our_sk, peer_pk)`, then feeds the shared secret through
//!    HKDF-SHA256 to produce a 256-bit AES key.
//! 3. **Encryption:** Every USB/IP message is encrypted with
//!    AES-256-GCM. The 12-byte nonce is sent alongside ciphertext.

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::digest;
use ring::hkdf;
use ring::rand::SecureRandom;

use std::fmt;

// ─── Constants ────────────────────────────────────────────────────

/// X25519 private key length in bytes.
pub const X25519_PRIVATE_KEY_LEN: usize = 32;

/// X25519 public key length in bytes.
pub const X25519_PUBLIC_KEY_LEN: usize = 32;

/// AES-256 key length (256-bit).
pub const AES_256_KEY_LEN: usize = 32;

/// AES-GCM nonce length (96-bit).
pub const NONCE_LEN: usize = 12;

/// AES-GCM tag length (128-bit).
pub const TAG_LEN: usize = 16;

/// HKDF output key length.
pub const HKDF_OUTPUT_LEN: usize = 32;

/// HKDF salt length.
pub const HKDF_SALT_LEN: usize = 32;

/// Default HKDF salt (32 zero bytes).
pub const DEFAULT_HKDF_SALT: [u8; HKDF_SALT_LEN] = [0u8; HKDF_SALT_LEN];

/// HKDF info string for domain separation.
pub const HKDF_INFO: &[u8] = b"usb-passthrough-session-key-v1";

// ─── Errors ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum CryptoError {
    KeyGeneration,
    EcdhAgreement,
    KeyDerivation,
    EncryptError,
    DecryptError,
    InvalidKeyHex,
    InvalidKeyLength { expected: usize, got: usize },
    InvalidInputLength,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyGeneration => write!(f, "key generation failed"),
            Self::EcdhAgreement => write!(f, "ECDH agreement failed"),
            Self::KeyDerivation => write!(f, "HKDF key derivation failed"),
            Self::EncryptError => write!(f, "encryption failed"),
            Self::DecryptError => write!(f, "decryption failed"),
            Self::InvalidKeyHex => write!(f, "invalid key hex string"),
            Self::InvalidKeyLength { expected, got } => {
                write!(f, "expected key length {expected}, got {got}")
            },
            Self::InvalidInputLength => write!(f, "invalid input length"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// Result type for crypto operations.
pub type CryptoResult<T> = Result<T, CryptoError>;

// ─── X25519 Field Arithmetic (Pure Rust) ─────────────────────────
//
// Implements X25519 (Curve25519 scalar multiplication on the
// Montgomery curve).  The field is GF(2²⁵⁵ − 19).  We use the
// standard Montgomery ladder to compute v ≡ u-coordinate of
// [scalar]B, where B = 9 is the base-point u-coordinate.
//
// This is the well-known algorithm from Daniel J. Bernstein's
// Curve25519 paper (https://cr.yp.to/ecdh.html).

/// The prime 2²⁵⁵ − 19.
const P: [u64; 5] = [
    0xffffffffffffffed,
    0xffffffffffffffff,
    0xffffffffffffffff,
    0xffffffffffffffff,
    0x7fffffffffffffff,
];

/// `a24 = 121665`, the curve constant (a − 2) / 4.
const A24: u64 = 121665;

/// Reduce a 320-bit (5×64-bit) value modulo 2²⁵⁵ − 19.
#[inline]
fn reduce(mut a: [u64; 5]) -> [u64; 5] {
    let c = a[4] >> 63;
    a[4] &= 0x7fffffffffffffff;

    // Multiply by 19 and add back.
    let carry = 19u128 * c as u128;
    let mut r = a[0] as u128 + carry;
    a[0] = r as u64;
    r >>= 64;
    r += a[1] as u128;
    a[1] = r as u64;
    r >>= 64;
    r += a[2] as u128;
    a[2] = r as u64;
    r >>= 64;
    r += a[3] as u128;
    a[3] = r as u64;
    r >>= 64;
    r += a[4] as u128;
    a[4] = r as u64;

    // Tighten: subtract p if >= p.
    let mut under = [0u64; 5];
    let mut borrow = 0i128;
    for i in 0..5 {
        borrow = a[i] as i128 - P[i] as i128 - borrow;
        under[i] = borrow as u64;
        borrow = (borrow >> 64) as i128;
    }
    let mask = !(borrow >> 64) as u64; // all-1 if a < p
    for i in 0..5 {
        a[i] = (a[i] & !mask) | (under[i] & mask);
    }
    a
}

/// Add two field elements (a + b) mod p.
fn add(a: &[u64; 5], b: &[u64; 5]) -> [u64; 5] {
    let mut r = [0u64; 5];
    let mut carry = 0u128;
    for i in 0..5 {
        carry += a[i] as u128 + b[i] as u128;
        r[i] = carry as u64;
        carry >>= 64;
    }
    reduce(r)
}

/// Subtract two field elements (a − b) mod p.
fn sub(a: &[u64; 5], b: &[u64; 5]) -> [u64; 5] {
    // Add p to a before subtracting to avoid underflow.
    let mut carry = 0u128;
    let mut r = [0u64; 5];
    for i in 0..5 {
        carry += (a[i] as u128).wrapping_add(P[i] as u128).wrapping_sub(b[i] as u128);
        r[i] = carry as u64;
        carry = (carry >> 64) as u128 + (carry >> 127) as u128; // sign extend
        if carry > 0 {
            carry -= 1;
        }
        carry = 0;
    }
    // Simpler approach: a + p - b
    let bp = 0u128;
    let mut r = [0u64; 5];
    let mut carry: u128 = 0;
    for i in 0..5 {
        carry += a[i] as u128 + P[i] as u128 - b[i] as u128;
        r[i] = carry as u64;
        carry >>= 64;
    }
    reduce(r)
}

/// Multiply two field elements (a × b) mod p.
fn mul(a: &[u64; 5], b: &[u64; 5]) -> [u64; 5] {
    // Schoolbook multiplication into 10 limbs, then reduce.
    let mut t = [0u64; 10];
    for i in 0..5 {
        let mut carry = 0u128;
        for j in 0..5 {
            carry += t[i + j] as u128 + a[i] as u128 * b[j] as u128;
            t[i + j] = carry as u64;
            carry >>= 64;
        }
        t[i + 5] = carry as u64;
    }

    // Reduce from 10 to 5 limbs using 2²⁵⁵ ≡ 19 (mod p).
    let mut r = [0u64; 5];
    let mut carry = 0u128;
    for i in 0..5 {
        carry += t[i] as u128 + 19u128 * t[i + 5] as u128;
        r[i] = carry as u64;
        carry >>= 64;
    }
    reduce(r)
}

/// Square a field element (a²) mod p.
fn sqr(a: &[u64; 5]) -> [u64; 5] {
    mul(a, a)
}

/// Invert a field element (a⁻¹) mod p using Fermat's little theorem:
/// a⁻¹ = a^(p−2) where p = 2²⁵⁵ − 19.
fn inv(a: &[u64; 5]) -> [u64; 5] {
    // Use the standard addition chain for 2²⁵⁵ − 21.
    let mut z = *a;
    let mut t0 = sqr(&z);
    let mut t1 = sqr(&t0);
    t1 = mul(&t1, &z);
    let mut t2 = sqr(&t1);
    t2 = mul(&t2, &z);
    let mut t3 = sqr(&t2);
    t3 = sqr(&t3);
    t3 = sqr(&t3);
    t3 = mul(&t3, &t2);
    let mut t4 = sqr(&t3);
    t4 = sqr(&t4);
    t4 = sqr(&t4);
    t4 = sqr(&t4);
    t4 = mul(&t4, &t3);
    let mut t5 = sqr(&t4);
    for _ in 0..4 {
        t5 = sqr(&t5);
    }
    t5 = mul(&t5, &t4);
    let mut t6 = sqr(&t5);
    for _ in 0..5 {
        t6 = sqr(&t6);
    }
    t6 = mul(&t6, &t5);
    let mut t7 = sqr(&t6);
    for _ in 0..6 {
        t7 = sqr(&t7);
    }
    t7 = mul(&t7, &t6);
    let mut t8 = sqr(&t7);
    for _ in 0..7 {
        t8 = sqr(&t8);
    }
    t8 = mul(&t8, &t7);
    t0 = sqr(&t8);
    for _ in 0..8 {
        t0 = sqr(&t0);
    }
    t0 = mul(&t0, &t8);
    for _ in 0..4 {
        t0 = sqr(&t0);
    }
    t0 = mul(&t0, &t4);
    for _ in 0..9 {
        t0 = sqr(&t0);
    }
    t0 = mul(&t0, &t5);
    for _ in 0..10 {
        t0 = sqr(&t0);
    }
    t0 = mul(&t0, &t6);
    for _ in 0..5 {
        t0 = sqr(&t0);
    }
    t0 = mul(&t0, &t3);
    for _ in 0..6 {
        t0 = sqr(&t0);
    }
    t0 = mul(&t0, &t2);
    for _ in 0..3 {
        t0 = sqr(&t0);
    }
    t0 = mul(&t0, &t1);
    for _ in 0..2 {
        t0 = sqr(&t0);
    }
    t0 = mul(&t0, &z);
    for _ in 0..63 {
        t0 = sqr(&t0);
    }
    mul(&t0, &z)
}

/// Decode a 32-byte little-endian u-coordinate into a field element.
fn decode_u(bytes: &[u8; 32]) -> [u64; 5] {
    let mut r = [0u64; 5];
    for i in 0..5 {
        let off = i * 8;
        r[i] = u64::from_le_bytes([
            bytes[off],
            bytes[off + 1],
            bytes[off + 2],
            bytes[off + 3],
            bytes[off + 4],
            bytes[off + 5],
            bytes[off + 6],
            bytes[off + 7],
        ]);
    }
    r[4] &= 0x7fffffffffffffff; // clear the high bit
    r
}

/// Encode a field element into 32 little-endian bytes.
fn encode_u(a: &[u64; 5]) -> [u8; 32] {
    let a = reduce(*a);
    let mut out = [0u8; 32];
    for i in 0..5 {
        let off = i * 8;
        out[off..off + 8].copy_from_slice(&a[i].to_le_bytes());
    }
    out
}

/// Clamp a 32-byte scalar per RFC 7748:
///  - Clear the 3 low-order bits.
///  - Set the high-order bit of the last byte.
///  - Clear the second-highest bit of the last byte.
fn clamp(scalar: &mut [u8; 32]) {
    scalar[0] &= 0xf8;
    scalar[31] &= 0x7f;
    scalar[31] |= 0x40;
}

/// The X25519 function: given a 32-byte scalar `k` (clamped internally)
/// and a 32-byte u-coordinate `u`, returns [k]u.
///
/// Uses the Montgomery ladder in constant time (data-independent
/// control flow), running in O(log p) field ops.
fn x25519(k: &[u8; 32], u: &[u8; 32]) -> [u8; 32] {
    let mut scalar = *k;
    clamp(&mut scalar);

    let u_fe = decode_u(u);

    // Montgomery ladder variables: x_2, z_2, x_3, z_3
    let mut x_2 = [1u64, 0, 0, 0, 0];
    let mut z_2 = [0u64, 0, 0, 0, 0];
    let mut x_3 = u_fe;
    let mut z_3 = [1u64, 0, 0, 0, 0];

    let mut swap = 0u64;

    for i in (0..255).rev() {
        let bit = ((scalar[i >> 3] >> (i & 7)) & 1) as u64;
        swap ^= bit;
        // Conditional swap
        let s = swap.wrapping_neg();
        for j in 0..5 {
            let t = s & (x_2[j] ^ x_3[j]);
            x_2[j] ^= t;
            x_3[j] ^= t;
            let t = s & (z_2[j] ^ z_3[j]);
            z_2[j] ^= t;
            z_3[j] ^= t;
        }
        swap = bit;

        let a = add(&x_2, &z_2);
        let aa = sqr(&a);
        let b = sub(&x_2, &z_2);
        let bb = sqr(&b);
        let c = add(&x_3, &z_3);
        let d = sub(&x_3, &z_3);
        let da = mul(&d, &a);
        let cb = mul(&c, &b);
        x_3 = sqr(&add(&da, &cb));
        z_3 = mul(&u_fe, &sqr(&sub(&da, &cb)));
        x_2 = mul(&aa, &bb);
        let e = sub(&aa, &bb);
        z_2 = mul(&e, &add(&aa, &mul(&[A24, 0, 0, 0, 0], &e)));
    }

    // Final conditional swap
    let s = swap.wrapping_neg();
    for j in 0..5 {
        let t = s & (x_2[j] ^ x_3[j]);
        x_2[j] ^= t;
        x_3[j] ^= t;
        let t = s & (z_2[j] ^ z_3[j]);
        z_2[j] ^= t;
        z_3[j] ^= t;
    }

    // Return x_2 * z_2⁻¹
    let result = mul(&x_2, &inv(&z_2));
    encode_u(&result)
}

// ─── Public API ───────────────────────────────────────────────────

/// An X25519 key pair.
#[derive(Debug)]
pub struct KeyPair {
    pub private: [u8; X25519_PRIVATE_KEY_LEN],
    pub public: [u8; X25519_PUBLIC_KEY_LEN],
}

impl KeyPair {
    /// Generate a new random X25519 key pair.
    pub fn generate() -> CryptoResult<Self> {
        let rng = ring::rand::SystemRandom::new();
        let mut private = [0u8; X25519_PRIVATE_KEY_LEN];
        rng.fill(&mut private).map_err(|_| CryptoError::KeyGeneration)?;

        // Compute public key = X25519(private, basepoint)
        let basepoint = [9u8; 32];
        let public = x25519(&private, &basepoint);

        Ok(Self { private, public })
    }

    /// Create a key pair from an existing 32-byte private key.
    pub fn from_private(private: [u8; X25519_PRIVATE_KEY_LEN]) -> Self {
        let basepoint = [9u8; 32];
        let public = x25519(&private, &basepoint);
        Self { private, public }
    }

    /// Encode the public key as a hex string.
    pub fn public_hex(&self) -> String {
        hex_encode(&self.public)
    }

    /// Encode the private key as a hex string.
    pub fn private_hex(&self) -> String {
        hex_encode(&self.private)
    }
}

/// Compute an X25519 shared secret.
///
/// `our_private` is our 32-byte private key (raw, unclamped).
/// `peer_public` is the peer's 32-byte public key.
///
/// Returns the 32-byte shared secret.
pub fn ecdh(our_private: &[u8; 32], peer_public: &[u8; 32]) -> [u8; 32] {
    x25519(our_private, peer_public)
}

/// Derive a 256-bit session key from an ECDH shared secret using
/// HKDF-SHA256.
///
/// - `shared_secret`: 32 bytes from `ecdh()`.
/// - `salt`: Optional 32-byte salt (use `DEFAULT_HKDF_SALT` if None).
/// - `info`: Context string (use `HKDF_INFO`).
///
/// Returns a 32-byte AES-256 key.
pub fn derive_session_key(
    shared_secret: &[u8; 32],
    salt: Option<&[u8; HKDF_SALT_LEN]>,
    info: &[u8],
) -> CryptoResult<[u8; HKDF_OUTPUT_LEN]> {
    let salt = salt.unwrap_or(&DEFAULT_HKDF_SALT);

    let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, salt.as_slice());
    let prk = salt.extract(shared_secret);

    let mut out = [0u8; HKDF_OUTPUT_LEN];
    let okm = prk.expand(&[info], HkdfKeyType).map_err(|_| CryptoError::KeyDerivation)?;
    okm.fill(&mut out).map_err(|_| CryptoError::KeyDerivation)?;

    Ok(out)
}

/// HKDF key type for 32-byte output.
struct HkdfKeyType;

impl hkdf::KeyType for HkdfKeyType {
    fn len(&self) -> usize {
        HKDF_OUTPUT_LEN
    }
}

// ─── AES-256-GCM ──────────────────────────────────────────────────

/// Encrypt `plaintext` with AES-256-GCM.
///
/// `key` must be 32 bytes.  Returns `[nonce (12) | ciphertext | tag (16)]`.
pub fn encrypt(key: &[u8; AES_256_KEY_LEN], plaintext: &[u8]) -> CryptoResult<Vec<u8>> {
    let rng = ring::rand::SystemRandom::new();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes).map_err(|_| CryptoError::EncryptError)?;

    let unbound_key = UnboundKey::new(&AES_256_GCM, key).map_err(|_| CryptoError::EncryptError)?;
    let key = LessSafeKey::new(unbound_key);

    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut in_out = plaintext.to_vec();
    // ring requires extra space for the tag
    in_out.resize(plaintext.len() + TAG_LEN, 0);

    key.seal_in_place_separate_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| CryptoError::EncryptError)?;

    let mut result = Vec::with_capacity(NONCE_LEN + in_out.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&in_out);
    Ok(result)
}

/// Decrypt an AES-256-GCM ciphertext produced by `encrypt()`.
///
/// Input format: `[nonce (12) | ciphertext | tag (16)]`.
pub fn decrypt(key: &[u8; AES_256_KEY_LEN], ciphertext: &[u8]) -> CryptoResult<Vec<u8>> {
    if ciphertext.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::InvalidInputLength);
    }

    let nonce_bytes: [u8; NONCE_LEN] =
        ciphertext[..NONCE_LEN].try_into().map_err(|_| CryptoError::InvalidInputLength)?;

    let unbound_key = UnboundKey::new(&AES_256_GCM, key).map_err(|_| CryptoError::DecryptError)?;
    let key = LessSafeKey::new(unbound_key);

    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut in_out = ciphertext[NONCE_LEN..].to_vec();

    let plaintext = key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| CryptoError::DecryptError)?;

    Ok(plaintext.to_vec())
}

/// Encrypt a USB/IP message (header + payload) in a single operation.
/// The AAD is the 8-byte USB/IP header, and the plaintext is the
/// message body.  Format: `[nonce (12) | aad_len (2, BE) | aad | ciphertext | tag]`.
pub fn encrypt_usbip_message(
    key: &[u8; AES_256_KEY_LEN],
    header: &[u8; 8],
    body: &[u8],
) -> CryptoResult<Vec<u8>> {
    let rng = ring::rand::SystemRandom::new();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes).map_err(|_| CryptoError::EncryptError)?;

    let unbound_key = UnboundKey::new(&AES_256_GCM, key).map_err(|_| CryptoError::EncryptError)?;
    let key = LessSafeKey::new(unbound_key);
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = body.to_vec();
    in_out.resize(body.len() + TAG_LEN, 0);

    key.seal_in_place_separate_tag(nonce, Aad::from(header.as_slice()), &mut in_out)
        .map_err(|_| CryptoError::EncryptError)?;

    let mut result = Vec::with_capacity(NONCE_LEN + in_out.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&in_out);
    Ok(result)
}

/// Decrypt a message produced by `encrypt_usbip_message()`.
pub fn decrypt_usbip_message(
    key: &[u8; AES_256_KEY_LEN],
    header: &[u8; 8],
    data: &[u8],
) -> CryptoResult<Vec<u8>> {
    if data.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::InvalidInputLength);
    }

    let nonce_bytes: [u8; NONCE_LEN] =
        data[..NONCE_LEN].try_into().map_err(|_| CryptoError::InvalidInputLength)?;

    let unbound_key = UnboundKey::new(&AES_256_GCM, key).map_err(|_| CryptoError::DecryptError)?;
    let key = LessSafeKey::new(unbound_key);
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = data[NONCE_LEN..].to_vec();
    let plaintext = key
        .open_in_place(nonce, Aad::from(header.as_slice()), &mut in_out)
        .map_err(|_| CryptoError::DecryptError)?;

    Ok(plaintext.to_vec())
}

// ─── Hex Encoding Helpers ─────────────────────────────────────────

/// Encode bytes as lowercase hex.
fn hex_encode(bytes: &[u8]) -> String {
    let hex_chars: &[u8] = b"0123456789abcdef";
    let mut out = Vec::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(hex_chars[(b >> 4) as usize]);
        out.push(hex_chars[(b & 0x0f) as usize]);
    }
    unsafe { String::from_utf8_unchecked(out) }
}

/// Decode a hex string to bytes.
fn hex_decode(hex: &str) -> Result<Vec<u8>, CryptoError> {
    if hex.len() % 2 != 0 {
        return Err(CryptoError::InvalidKeyHex);
    }
    let hex = hex.as_bytes();
    let mut out = Vec::with_capacity(hex.len() / 2);
    for chunk in hex.chunks(2) {
        let hi = from_hex(chunk[0])?;
        let lo = from_hex(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

#[inline]
fn from_hex(b: u8) -> Result<u8, CryptoError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(CryptoError::InvalidKeyHex),
    }
}

// ─── High-Level JNI-Friendly API ──────────────────────────────────

/// Generate an X25519 key pair, returning `(public_hex, private_hex)`.
pub fn generate_key_pair_hex() -> CryptoResult<(String, String)> {
    let kp = KeyPair::generate()?;
    Ok((kp.public_hex(), kp.private_hex()))
}

/// Derive a 32-byte AES-256 session key from our private key hex and
/// the peer's public key hex.
pub fn derive_session_key_hex(
    our_private_hex: &str,
    peer_public_hex: &str,
) -> CryptoResult<[u8; AES_256_KEY_LEN]> {
    let private_bytes = hex_decode(our_private_hex)?;
    let public_bytes = hex_decode(peer_public_hex)?;

    if private_bytes.len() != X25519_PRIVATE_KEY_LEN {
        return Err(CryptoError::InvalidKeyLength {
            expected: X25519_PRIVATE_KEY_LEN,
            got: private_bytes.len(),
        });
    }
    if public_bytes.len() != X25519_PUBLIC_KEY_LEN {
        return Err(CryptoError::InvalidKeyLength {
            expected: X25519_PUBLIC_KEY_LEN,
            got: public_bytes.len(),
        });
    }

    let mut private_arr = [0u8; X25519_PRIVATE_KEY_LEN];
    let mut public_arr = [0u8; X25519_PUBLIC_KEY_LEN];
    private_arr.copy_from_slice(&private_bytes);
    public_arr.copy_from_slice(&public_bytes);

    let shared_secret = ecdh(&private_arr, &public_arr);
    derive_session_key(&shared_secret, None, HKDF_INFO)
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let kp = KeyPair::generate().unwrap();
        assert_eq!(kp.private.len(), 32);
        assert_eq!(kp.public.len(), 32);
        assert_ne!(kp.private, [0u8; 32]);
        assert_ne!(kp.public, [0u8; 32]);
    }

    #[test]
    fn test_ecdh_agreement() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let shared_alice = ecdh(&alice.private, &bob.public);
        let shared_bob = ecdh(&bob.private, &alice.public);

        assert_eq!(shared_alice, shared_bob);
    }

    #[test]
    fn test_encrypt_decrypt() {
        let kp = KeyPair::generate().unwrap();
        let shared = ecdh(&kp.private, &kp.public);
        let key = derive_session_key(&shared, None, HKDF_INFO).unwrap();

        let plaintext = b"Hello, USB/IP!";
        let encrypted = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), &decrypted);
    }

    #[test]
    fn test_hex_roundtrip() {
        let kp = KeyPair::generate().unwrap();
        let pub_hex = kp.public_hex();
        let priv_hex = kp.private_hex();

        assert_eq!(pub_hex.len(), 64);
        assert_eq!(priv_hex.len(), 64);

        let pub_decoded = hex_decode(&pub_hex).unwrap();
        let priv_decoded = hex_decode(&priv_hex).unwrap();
        assert_eq!(pub_decoded, kp.public);
        assert_eq!(priv_decoded, kp.private);
    }

    #[test]
    fn test_derive_session_key_hex() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let key_a = derive_session_key_hex(&alice.private_hex(), &bob.public_hex()).unwrap();
        let key_b = derive_session_key_hex(&bob.private_hex(), &alice.public_hex()).unwrap();

        assert_eq!(key_a, key_b);
        assert_eq!(key_a.len(), 32);
    }

    #[test]
    fn test_usbip_message_encrypt_decrypt() {
        let kp = KeyPair::generate().unwrap();
        let shared = ecdh(&kp.private, &kp.public);
        let key = derive_session_key(&shared, None, HKDF_INFO).unwrap();

        let header = [0u8; 8];
        let body = b"\x01\x02\x03\x04";
        let encrypted = encrypt_usbip_message(&key, &header, body).unwrap();
        let decrypted = decrypt_usbip_message(&key, &header, &encrypted).unwrap();

        assert_eq!(body.as_slice(), &decrypted);
    }

    #[test]
    fn test_x25519_test_vector() {
        // RFC 7748 Section 6.1
        let scalar =
            hex_decode("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4").unwrap();
        let u_coord =
            hex_decode("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c").unwrap();

        let mut s = [0u8; 32];
        s.copy_from_slice(&scalar);
        let mut u = [0u8; 32];
        u.copy_from_slice(&u_coord);
        let result = x25519(&s, &u);

        let expected =
            hex_decode("c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552").unwrap();

        assert_eq!(result.as_slice(), expected.as_slice());
    }
}
