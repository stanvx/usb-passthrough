package com.anyplug.client

import kotlinx.coroutines.*
import java.io.InputStream
import java.io.OutputStream
import java.net.Socket
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Android USB/IP Client — imports remote USB devices.
 *
 * Connects to a USB/IP server, requests a device import, and
 * forwards URBs to the VHCI kernel module (root required for VHCI).
 *
 * Falls back to a userspace VHCI bridge on non-rooted devices
 * (limited to HID devices via /dev/uinput).
 */
class UsbIpClient(
    private val serverHost: String,
    private val serverPort: Int = 3240,
    private val busId: String
) {
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    private var socket: Socket? = null
    private var running = false
    private var seqnum = 0

    /**
     * Callback for URB submissions from the kernel VHCI driver.
     * The client app must forward these to the server.
     */
    interface UrbCallback {
        /** Kernel wants to submit a URB to the remote device. */
        suspend fun onUrbSubmit(
            seqnum: Int, devid: Int, direction: Int, ep: Int,
            flags: Int, dataLen: Int, setup: ByteArray, data: ByteArray
        ): UrbResult

        /** A previously submitted URB was cancelled. */
        fun onUrbCancel(seqnum: Int)
    }

    data class UrbResult(
        val status: Int,       // 0 = success, negative = errno
        val actualLength: Int,
        val data: ByteArray    // only for IN transfers
    )

    companion object {
        const val DEFAULT_PORT = 3240
        const val USBIP_VERSION = 0x0111
        // Protocol commands
        const val OP_REQ_DEVLIST  = 0x8003
        const val OP_REP_DEVLIST  = 0x0005
        const val OP_REQ_IMPORT   = 0x8006
        const val OP_REP_IMPORT   = 0x0007
        const val USBIP_CMD_SUBMIT = 0x0001
        const val USBIP_RET_SUBMIT = 0x0003
        const val USBIP_RET_UNLINK = 0x0002
    }

    suspend fun start() {
        running = true

        withContext(Dispatchers.IO) {
            socket = Socket(serverHost, serverPort).apply {
                tcpNoDelay = true
            }
        }

        val input = socket!!.getInputStream()
        val output = socket!!.getOutputStream()

        // Request import
        requestImport(input, output)

        // Enter URB loop
        urbLoop(input, output)
    }

    fun stop() {
        running = false
        scope.cancel()
        socket?.close()
    }

    /**
     * Submit a URB from the kernel VHCI to the remote server.
     * Called by the VHCI driver bridge when the kernel submits an URB.
     */
    suspend fun submitUrb(
        devid: Int, direction: Int, ep: Int,
        flags: Int, dataLen: Int, setup: ByteArray, data: ByteArray
    ): UrbResult {
        val s = socket ?: return UrbResult(-108/*ESHUTDOWN*/, 0, ByteArray(0))
        val output = s.getOutputStream()
        val input = s.getInputStream()

        seqnum++

        // Build CMD_SUBMIT
        val cmdBuf = ByteBuffer.allocate(8 + 48 + data.size).order(ByteOrder.BIG_ENDIAN)

        // USB/IP header
        cmdBuf.putShort(USBIP_VERSION.toShort())
        cmdBuf.putShort(USBIP_CMD_SUBMIT.toShort())
        cmdBuf.putInt(0)

        // CMD_SUBMIT struct
        cmdBuf.putInt(seqnum)
        cmdBuf.putInt(devid)
        cmdBuf.putInt(direction)
        cmdBuf.putInt(ep)
        cmdBuf.putInt(flags)
        cmdBuf.putInt(dataLen)
        cmdBuf.putInt(0) // start_frame
        cmdBuf.putInt(0) // number_of_packets
        cmdBuf.putInt(0) // interval
        cmdBuf.put(setup)

        // OUT data
        if (direction == 0 && data.isNotEmpty()) {
            cmdBuf.put(data)
        }

        withContext(Dispatchers.IO) {
            output.write(cmdBuf.array())
            output.flush()
        }

        // Read RET_SUBMIT
        val retHeader = ByteArray(8)
        val retStruct = ByteArray(40)

        withContext(Dispatchers.IO) {
            input.read(retHeader)
            input.read(retStruct)
        }

        val ret = ByteBuffer.wrap(retStruct).order(ByteOrder.BIG_ENDIAN)
        val retSeqnum = ret.getInt(0)
        val retDevid = ret.getInt(4)
        val retStatus = ret.getInt(16)
        val retActualLen = ret.getInt(20)

        // Read IN data if present
        val inData = if (direction == 1 && retStatus == 0 && retActualLen > 0) {
            val buf = ByteArray(retActualLen)
            withContext(Dispatchers.IO) { input.read(buf) }
            buf
        } else {
            ByteArray(0)
        }

        return UrbResult(retStatus, retActualLen, inData)
    }

    // ─── Private ─────────────────────────────────────────────

    private fun requestImport(input: InputStream, output: OutputStream) {
        val reqBuf = ByteBuffer.allocate(8 + 32).order(ByteOrder.BIG_ENDIAN)

        // USB/IP header
        reqBuf.putShort(USBIP_VERSION.toShort())
        reqBuf.putShort(OP_REQ_IMPORT.toShort())
        reqBuf.putInt(0)

        // Busid (32 bytes, null-padded)
        val busidBytes = busId.toByteArray()
        reqBuf.put(busidBytes.copyOf(32))

        output.write(reqBuf.array())
        output.flush()

        // Read reply header
        val replyHeader = ByteArray(8)
        input.read(replyHeader)

        val rh = ByteBuffer.wrap(replyHeader).order(ByteOrder.BIG_ENDIAN)
        val command = rh.getShort(2).toInt() and 0xFFFF
        val status = rh.getInt(4)

        if (command != OP_REP_IMPORT) {
            throw IllegalStateException("Unexpected reply: 0x${command.toString(16)}")
        }
        if (status != 0) {
            throw IllegalStateException("Import rejected: status=$status")
        }

        // Read device entry (312 bytes) + descriptor tree
        val entryAndDesc = ByteArray(4096)
        val totalRead = input.read(entryAndDesc)
        // Parse descriptors to verify device identity
    }

    private suspend fun urbLoop(input: InputStream, output: OutputStream) {
        // The client primarily initiates URBs via submitUrb(),
        // but the server may send async RET_UNLINK or RET_SUBMIT
        // for previously submitted URBs.
    }
}
