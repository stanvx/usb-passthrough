package com.anyplug

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbManager
import android.os.Build
import android.util.Log

/** The intent action for requesting USB device permission. */
private const val ACTION_USB_PERMISSION = "android.hardware.usb.action.USB_PERMISSION"

/**
 * Handles the USB permission request flow via [UsbManager.requestPermission].
 *
 * Android does not auto-grant permission for every device, even when a
 * [android.hardware.usb.action.USB_DEVICE_ATTACHED] intent filter exists.
 * This helper registers a [BroadcastReceiver] for the
 * [ACTION_USB_PERMISSION] action and invokes a callback when
 * the user grants (or denies) permission.
 *
 * Usage:
 * ```kotlin
 * val handler = UsbPermissionHandler(context)
 * handler.requestPermission(usbDevice) { granted ->
 *     if (granted) { /* open + claim device */ }
 * }
 * handler.unregister() // in onDestroy / onStop
 * ```
 */
class UsbPermissionHandler(private val context: Context) {

    private var pendingRequest: UsbDevice? = null
    private var pendingCallback: ((Boolean) -> Unit)? = null
    private var registered = false

    private val permissionReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (ACTION_USB_PERMISSION != intent.action) return

            synchronized(this@UsbPermissionHandler) {
                val device: UsbDevice? = intent.getParcelableExtra(UsbManager.EXTRA_DEVICE)
                val granted = intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false)

                // Only handle the device we asked about
                if (device != null && device == pendingRequest) {
                    Log.d(TAG, "USB permission ${if (granted) "granted" else "denied"} for ${device.productName}")
                    pendingCallback?.invoke(granted)
                    pendingRequest = null
                    pendingCallback = null
                }
            }
        }
    }

    /**
     * Request USB permission for [device]. The [onResult] callback receives
     * true if the user granted permission, false otherwise.
     *
     * Only one permission request can be in flight at a time. If a request is
     * already pending, this call is a no-op.
     */
    fun requestPermission(device: UsbDevice, onResult: (Boolean) -> Unit) {
        synchronized(this) {
            if (pendingRequest != null) {
                Log.w(TAG, "Permission request already in progress, ignoring duplicate")
                return
            }

            pendingRequest = device
            pendingCallback = onResult
            registerReceiver()

            val usbManager = context.getSystemService(Context.USB_SERVICE) as UsbManager
            val intent = Intent(ACTION_USB_PERMISSION)
            val pendingIntent = PendingIntent.getBroadcast(
                context,
                PERMISSION_REQUEST_CODE,
                intent,
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )

            usbManager.requestPermission(device, pendingIntent)
        }
    }

    /**
     * Unregister the broadcast receiver. Call from onDestroy / onStop
     * to prevent leaks.
     */
    fun unregister() {
        synchronized(this) {
            if (registered) {
                try {
                    context.unregisterReceiver(permissionReceiver)
                } catch (_: IllegalArgumentException) {
                    // Already unregistered
                }
                registered = false
            }
            pendingRequest = null
            pendingCallback = null
        }
    }

    private fun registerReceiver() {
        if (registered) return
        val filter = IntentFilter(ACTION_USB_PERMISSION)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.registerReceiver(permissionReceiver, filter,
                Context.RECEIVER_NOT_EXPORTED)
        } else {
            context.registerReceiver(permissionReceiver, filter)
        }
        registered = true
    }

    companion object {
        private const val TAG = "UsbPermissionHandler"
        private const val PERMISSION_REQUEST_CODE = 0x1001
    }
}
