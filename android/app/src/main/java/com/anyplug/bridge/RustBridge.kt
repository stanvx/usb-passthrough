package com.anyplug.bridge

/**
 * JNI bridge to the Rust usbip-android native library.
 *
 * The Rust library handles:
 *  - USB/IP protocol parsing (shared with desktop Rust crates)
 *  - URB buffer pool allocation
 *  - AES-GCM encryption/decryption
 *  - CRC32 validation
 *
 * Kotlin code calls these native functions for performance-critical
 * operations while the Android USB Host API handles actual USB I/O.
 */
object RustBridge {
    private var loaded = false

    fun init() {
        if (!loaded) {
            try {
                System.loadLibrary("usbip_android")
                loaded = true
            } catch (e: UnsatisfiedLinkError) {
                // Native lib not available — fall back to pure Kotlin path
                // (supported for non-encrypted, basic operation)
            }
        }
    }

    /** Check if native library is loaded. */
    fun isLoaded(): Boolean = loaded

    // ─── Protocol Functions ──────────────────────────────────

    /**
     * Parse a USB/IP device list reply.
     * Returns JSON array of device objects.
     */
    external fun parseDeviceListReply(payload: ByteArray): String

    /**
     * Parse a USB/IP import reply, extracting device entry + descriptor tree.
     */
    external fun parseImportReply(payload: ByteArray): ImportReply

    /**
     * Build a CMD_SUBMIT packet.
     */
    external fun buildCmdSubmit(
        seqnum: Int, devid: Int, direction: Int, ep: Int,
        flags: Int, dataLen: Int, setup: ByteArray, data: ByteArray
    ): ByteArray

    /**
     * Parse a RET_SUBMIT packet.
     */
    external fun parseRetSubmit(payload: ByteArray): RetSubmit

    // ─── Encryption Functions ────────────────────────────────

    /**
     * Generate an X25519 ECDH key pair.
     * Returns [public_key_hex, private_key_hex].
     */
    external fun generateKeyPair(): Array<String>

    /**
     * Derive an AES-256-GCM session key from ECDH + HKDF-SHA256.
     */
    external fun deriveSessionKey(
        ourPrivateKeyHex: String,
        peerPublicKeyHex: String
    ): ByteArray

    /**
     * Encrypt a USB/IP message with AES-256-GCM.
     * Returns [nonce + ciphertext + tag].
     */
    external fun encryptMessage(key: ByteArray, plaintext: ByteArray): ByteArray

    /**
     * Decrypt a USB/IP message with AES-256-GCM.
     */
    external fun decryptMessage(key: ByteArray, ciphertext: ByteArray): ByteArray

    // ─── Result Types ────────────────────────────────────────

    data class ImportReply(
        val vendorId: Int,
        val productId: Int,
        val busId: String,
        val speed: Int,
        val descriptors: ByteArray
    )

    data class RetSubmit(
        val seqnum: Int,
        val devid: Int,
        val status: Int,
        val actualLength: Int,
        val data: ByteArray
    )
}
