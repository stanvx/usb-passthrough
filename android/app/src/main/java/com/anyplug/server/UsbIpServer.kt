package com.anyplug.server

import android.content.Context
import android.hardware.usb.*
import kotlinx.coroutines.*
import com.anyplug.WakeLockManager
import java.io.InputStream
import java.io.OutputStream
import java.net.ServerSocket
import java.net.Socket
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Android USB/IP Server — exports locally-connected USB devices.
 *
 * Uses Android's USB Host API to claim a USB device and proxy all
 * USB transfers (bulk, control, interrupt) over TCP using the
 * USB/IP protocol.
 *
 * Protocol reference: see PROTOCOL.md
 */
class UsbIpServer(
    private val context: Context,
    private val deviceFilter: UsbDeviceFilter,
    private val wakeLockManager: WakeLockManager? = null,
    private val port: Int = DEFAULT_PORT
) {
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    private var serverSocket: ServerSocket? = null
    private var deviceConnection: UsbDeviceConnection? = null
    private var claimedDevice: UsbDevice? = null
    private var running = false

    data class UsbDeviceFilter(val vendorId: Int, val productId: Int)

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
        val usbManager = context.getSystemService(Context.USB_SERVICE) as UsbManager

        // Find and claim the target device
        claimedDevice = findDevice(usbManager)
            ?: throw IllegalStateException("Device not found: ${deviceFilter}")

        deviceConnection = claimDevice(usbManager, claimedDevice!!)
            ?: throw IllegalStateException("Failed to claim device")

        // Start TCP server
        serverSocket = ServerSocket(port)

        while (running) {
            val client = withContext(Dispatchers.IO) {
                serverSocket?.accept()
            } ?: break

            scope.launch {
                handleClient(client, usbManager)
            }
        }
    }

    fun stop() {
        running = false
        scope.cancel()
        serverSocket?.close()
        releaseDevice()
    }

    private fun findDevice(manager: UsbManager): UsbDevice? {
        for ((_, device) in manager.deviceList) {
            if (device.vendorId == deviceFilter.vendorId &&
                device.productId == deviceFilter.productId) {
                return device
            }
        }
        return null
    }

    private fun claimDevice(manager: UsbManager, device: UsbDevice): UsbDeviceConnection? {
        if (!manager.hasPermission(device)) {
            // Request permission — in practice, the Activity handles this
            return null
        }

        val connection = manager.openDevice(device) ?: return null

        // Claim all interfaces
        for (i in 0 until device.interfaceCount) {
            val iface = device.getInterface(i)
            if (!connection.claimInterface(iface, true)) {
                // Release previously claimed interfaces
                for (j in 0 until i) {
                    connection.releaseInterface(device.getInterface(j))
                }
                connection.close()
                return null
            }
        }

        return connection
    }

    private fun releaseDevice() {
        val conn = deviceConnection ?: return
        val device = claimedDevice ?: return

        for (i in 0 until device.interfaceCount) {
            conn.releaseInterface(device.getInterface(i))
        }
        conn.close()
        deviceConnection = null
        claimedDevice = null
    }

    // ─── Client Handler ──────────────────────────────────────

    private suspend fun handleClient(socket: Socket, usbManager: UsbManager) {
        val input = socket.getInputStream()
        val output = socket.getOutputStream()

        try {
            // Read USB/IP header (8 bytes)
            val headerBuf = ByteArray(8)
            if (!readExact(input, headerBuf)) return

            val command = ByteBuffer.wrap(headerBuf).order(ByteOrder.BIG_ENDIAN).getShort(2).toInt() and 0xFFFF

            when (command) {
                OP_REQ_DEVLIST -> handleDevList(output)
                OP_REQ_IMPORT -> handleImport(input, output)
            }
        } catch (e: Exception) {
            e.printStackTrace()
        } finally {
            socket.close()
        }
    }

    private fun handleDevList(output: OutputStream) {
        val device = claimedDevice ?: return

        // Build device entry (simplified)
        val entry = buildDeviceEntry(device)

        // Send OP_REP_DEVLIST
        val reply = ByteArray(8 + 4 + entry.size) // header + ndev + entry
        val buf = ByteBuffer.wrap(reply).order(ByteOrder.BIG_ENDIAN)
        buf.putShort(USBIP_VERSION.toShort())
        buf.putShort(OP_REP_DEVLIST.toShort())
        buf.putInt(0) // status = success
        buf.putInt(1) // ndev = 1
        buf.put(entry)

        output.write(reply)
        output.flush()
    }

    private fun handleImport(input: InputStream, output: OutputStream) {
        // Read busid (32 bytes)
        val busidBuf = ByteArray(32)
        if (!readExact(input, busidBuf)) return

        // Send OP_REP_IMPORT with device entry + descriptor tree
        val device = claimedDevice ?: return
        val entry = buildDeviceEntry(device)
        val descriptors = getRawDescriptors(device)

        val replyHeader = ByteArray(8)
        val headerBuf = ByteBuffer.wrap(replyHeader).order(ByteOrder.BIG_ENDIAN)
        headerBuf.putShort(USBIP_VERSION.toShort())
        headerBuf.putShort(OP_REP_IMPORT.toShort())
        headerBuf.putInt(0) // status = success

        output.write(replyHeader)
        output.write(entry)
        output.write(descriptors)
        output.flush()

        // Enter URB forwarding loop
        urbLoop(input, output, device)
    }

    // ─── URB Forwarding Loop ─────────────────────────────────

    private fun urbLoop(input: InputStream, output: OutputStream, device: UsbDevice) {
        val headerBuf = ByteArray(8)
        val conn = deviceConnection ?: return

        while (running) {
            if (!readExact(input, headerBuf)) break

            val buf = ByteBuffer.wrap(headerBuf).order(ByteOrder.BIG_ENDIAN)
            val command = buf.getShort(2).toInt() and 0xFFFF

            if (command != USBIP_CMD_SUBMIT) break

            // Acquire transfer wake lock before reading URB data
            wakeLockManager?.acquireTransferWakeLock()

            try {
                // Read CMD_SUBMIT header (48 bytes)
                val cmdBuf = ByteArray(48)
                if (!readExact(input, cmdBuf)) break

                val cmd = ByteBuffer.wrap(cmdBuf).order(ByteOrder.BIG_ENDIAN)
                val seqnum = cmd.getInt(0)
                val devid = cmd.getInt(4)
                val direction = cmd.getInt(8)
                val ep = cmd.getInt(12)
                val flags = cmd.getInt(16)
                val dataLen = cmd.getInt(20)
                val setup = ByteArray(8)
                cmd.position(40)
                cmd.get(setup)

                // Read data for OUT transfers
                val outData = ByteArray(dataLen)
                if (direction == 0 && dataLen > 0) { // OUT
                    if (!readExact(input, outData)) break
                }

                // Execute URB on real device
                try {
                    val (status, actualLen, inData) = executeUrb(
                        conn, device, ep, direction, flags, dataLen, setup, outData
                    )

                    // Send RET_SUBMIT
                    val retBuf = buildRetSubmit(seqnum, devid, ep, direction, status, actualLen, setup, inData)
                    output.write(retBuf)
                    output.flush()
                } catch (e: Exception) {
                    // Send error RET_SUBMIT
                    val retBuf = buildRetSubmit(seqnum, devid, ep, direction, -5 /* -EIO */, 0, setup, ByteArray(0))
                    output.write(retBuf)
                    output.flush()
                }
            } finally {
                // Release transfer wake lock after URB completes
                wakeLockManager?.releaseTransferWakeLock()
            }
        }
    }

    private fun executeUrb(
        conn: UsbDeviceConnection,
        device: UsbDevice,
        epAddr: Int,
        direction: Int,
        flags: Int,
        dataLen: Int,
        setup: ByteArray,
        outData: ByteArray
    ): Triple<Int, Int, ByteArray> {
        val epNumber = epAddr and 0x0F
        val isIn = (epAddr and 0x80) != 0

        // Find the endpoint object
        val endpoint = findEndpoint(device, epAddr)
            ?: return Triple(-19 /* -ENODEV */, 0, ByteArray(0))

        return if (isControlTransfer(setup)) {
            // Control transfer
            val bmRequestType = setup[0].toInt() and 0xFF
            val bRequest = setup[1].toInt() and 0xFF
            val wValue = ((setup[2].toInt() and 0xFF) or ((setup[3].toInt() and 0xFF) shl 8))
            val wIndex = ((setup[4].toInt() and 0xFF) or ((setup[5].toInt() and 0xFF) shl 8))
            val wLength = ((setup[6].toInt() and 0xFF) or ((setup[7].toInt() and 0xFF) shl 8))

            if ((bmRequestType and 0x80) != 0) {
                // IN
                val buf = ByteArray(wLength)
                val len = conn.controlTransfer(bmRequestType, bRequest, wValue, wIndex, buf, wLength, 5000)
                Triple(if (len >= 0) 0 else len, maxOf(0, len), buf.copyOf(maxOf(0, len)))
            } else {
                // OUT
                val len = conn.controlTransfer(bmRequestType, bRequest, wValue, wIndex, outData, outData.size, 5000)
                Triple(if (len >= 0) 0 else len, maxOf(0, len), ByteArray(0))
            }
        } else if (isIn) {
            // Bulk/Interrupt IN
            val maxSize = maxOf(dataLen, endpoint.maxPacketSize)
            val buf = ByteArray(maxSize)
            val len = conn.bulkTransfer(endpoint, buf, maxSize, 5000)
            Triple(if (len >= 0) 0 else len, maxOf(0, len), buf.copyOf(maxOf(0, len)))
        } else {
            // Bulk/Interrupt OUT
            val len = conn.bulkTransfer(endpoint, outData, outData.size, 5000)
            Triple(if (len >= 0) 0 else len, maxOf(0, len), ByteArray(0))
        }
    }

    private fun findEndpoint(device: UsbDevice, epAddr: Int): UsbEndpoint? {
        for (i in 0 until device.interfaceCount) {
            val iface = device.getInterface(i)
            for (j in 0 until iface.endpointCount) {
                val ep = iface.getEndpoint(j)
                if (ep.address == epAddr) return ep
            }
        }
        return null
    }

    private fun isControlTransfer(setup: ByteArray): Boolean {
        return setup.any { it != 0.toByte() }
    }

    // ─── Wire Format Builders ────────────────────────────────

    private fun buildDeviceEntry(device: UsbDevice): ByteArray {
        val entry = ByteBuffer.allocate(312).order(ByteOrder.BIG_ENDIAN)

        // Path (256 bytes, null-padded)
        val path = "/sys/bus/usb/devices/${device.deviceName}".toByteArray()
        entry.put(path.copyOf(256))

        // Busid (32 bytes, null-padded)
        val busid = "${device.deviceId}".toByteArray()
        entry.put(busid.copyOf(32))

        // Rest of fields
        entry.putInt(device.deviceId) // busnum
        entry.putInt(device.deviceId) // devnum
        entry.putInt(1) // speed (full)
        entry.putShort(device.vendorId.toShort())
        entry.putShort(device.productId.toShort())
        entry.putShort(0) // bcdDevice
        entry.put(device.deviceClass.toByte())
        entry.put(device.deviceSubclass.toByte())
        entry.put(device.deviceProtocol.toByte())
        entry.put(0) // bConfigurationValue
        entry.put(device.configurationCount.toByte())
        entry.put(0) // bNumInterfaces (filled below)

        // Count interfaces
        var numIfaces = 0
        for (i in 0 until device.interfaceCount) {
            numIfaces += device.getInterface(i).endpointCount
        }
        entry.put(304, numIfaces.toByte()) // offset 304 is bNumInterfaces

        return entry.array()
    }

    private fun getRawDescriptors(device: UsbDevice): ByteArray {
        // Build raw USB descriptor tree
        val tree = ByteArray(512) // pre-allocate
        val buf = ByteBuffer.wrap(tree).order(ByteOrder.LITTLE_ENDIAN)

        // Device descriptor (18 bytes)
        buf.put(18.toByte()) // bLength
        buf.put(1)  // bDescriptorType
        buf.putShort(0x0200) // bcdUSB 2.0
        buf.put(device.deviceClass.toByte())
        buf.put(device.deviceSubclass.toByte())
        buf.put(device.deviceProtocol.toByte())
        buf.put(64) // bMaxPacketSize0
        buf.putShort(device.vendorId.toShort())
        buf.putShort(device.productId.toShort())
        buf.putShort(0) // bcdDevice
        buf.put(0) // iManufacturer
        buf.put(0) // iProduct
        buf.put(0) // iSerialNumber
        buf.put(device.configurationCount.toByte())

        // Configuration + Interface + Endpoint descriptors
        // (simplified — real implementation would read from getRawDescriptors())
        // ...

        return tree.copyOf(buf.position())
    }

    private fun buildRetSubmit(
        seqnum: Int, devid: Int, ep: Int, direction: Int,
        status: Int, actualLen: Int, setup: ByteArray, inData: ByteArray
    ): ByteArray {
        val retBuf = ByteBuffer.allocate(8 + 40 + inData.size).order(ByteOrder.BIG_ENDIAN)

        // USB/IP header
        retBuf.putShort(USBIP_VERSION.toShort())
        retBuf.putShort(USBIP_RET_SUBMIT.toShort())
        retBuf.putInt(0) // status

        // RET_SUBMIT struct
        retBuf.putInt(seqnum)
        retBuf.putInt(devid)
        retBuf.putInt(direction)
        retBuf.putInt(ep)
        retBuf.putInt(status)
        retBuf.putInt(actualLen)
        retBuf.putInt(0) // start_frame
        retBuf.putInt(0) // number_of_packets
        retBuf.putInt(0) // error_count
        retBuf.put(setup)

        // Data (if IN and success)
        if (inData.isNotEmpty()) {
            retBuf.put(inData)
        }

        return retBuf.array()
    }

    // ─── I/O Helpers ─────────────────────────────────────────

    private fun readExact(input: InputStream, buf: ByteArray): Boolean {
        var offset = 0
        while (offset < buf.size) {
            val n = input.read(buf, offset, buf.size - offset)
            if (n < 0) return false
            offset += n
        }
        return true
    }
}
