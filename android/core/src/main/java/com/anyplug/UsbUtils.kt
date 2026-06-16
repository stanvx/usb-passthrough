package com.anyplug

import android.content.BroadcastReceiver
import android.content.Context
import android.content.IntentFilter
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbManager
import android.os.Build
import com.anyplug.model.LocalUsbDevice

/**
 * UsbManager extension — find a [UsbDevice] by its [LocalUsbDevice] model.
 */
fun UsbManager.findDevice(model: LocalUsbDevice): UsbDevice? =
    deviceList.entries.firstOrNull { (_, d) ->
        d.vendorId == model.vid && d.productId == model.pid
    }?.value

/**
 * UsbManager extension — enumerate attached devices into [LocalUsbDevice] models.
 * Includes the USB class code for mass-storage warnings.
 */
fun UsbManager.attachedDevices(): List<LocalUsbDevice> =
    deviceList.values.map { device ->
        LocalUsbDevice(
            name = device.productName ?: "USB Device ${device.deviceId}",
            vid = device.vendorId,
            pid = device.productId,
            deviceClass = device.deviceClass,
        )
    }

/**
 * Convenience accessor for the system USB service.
 */
val Context.usbManager: UsbManager
    get() = getSystemService(Context.USB_SERVICE) as UsbManager

/**
 * Register a [BroadcastReceiver] for the given [action] with the correct
 * RECEIVER_NOT_EXPORTED flag on API 33+.
 */
fun Context.registerReceiverSafely(
    receiver: BroadcastReceiver,
    action: String,
) {
    val filter = IntentFilter(action)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
    } else {
        registerReceiver(receiver, filter)
    }
}

/**
 * Result of parsing a "host:port" string.
 */
data class HostPort(val host: String, val port: Int = 3240)

/**
 * Parse a "host:port" string into [HostPort], defaulting to 3240 when
 * no port is provided or the port portion is unparseable.
 */
fun parseHostPort(input: String): HostPort {
    val parts = input.split(":")
    return HostPort(
        host = parts[0],
        port = if (parts.size > 1) parts[1].toIntOrNull() ?: 3240 else 3240,
    )
}
