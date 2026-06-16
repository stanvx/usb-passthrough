package com.anyplug.tv

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.hardware.usb.UsbManager
import android.os.Bundle
import android.os.IBinder
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.anyplug.AnyPlugService
import com.anyplug.bridge.RustBridge
import com.anyplug.model.DiscoveredServer
import com.anyplug.model.LocalUsbDevice
import com.anyplug.tv.theme.TvTheme
import com.anyplug.tv.ui.TvLeanbackScreen

/**
 * Android TV launcher activity with Leanback-optimised UI.
 *
 * Uses the dark [TvTheme] with enlarged typography and D-pad
 * focus management for TV remote operation.
 */
class TvMainActivity : ComponentActivity() {

    private var service: AnyPlugService? = null
    private var serviceBound = false

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, binder: IBinder?) {
            service = (binder as AnyPlugService.LocalBinder).getService()
            serviceBound = true
        }

        override fun onServiceDisconnected(name: ComponentName?) {
            service = null
            serviceBound = false
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Edge-to-edge for modern TV displays
        enableEdgeToEdge()

        // Initialize Rust JNI bridge
        RustBridge.init()

        // Bind to foreground service
        val intent = Intent(this, AnyPlugService::class.java)
        bindService(intent, serviceConnection, Context.BIND_AUTO_CREATE)

        setContent {
            TvTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    var isRunning by remember { mutableStateOf(false) }
                    var modeText by remember { mutableStateOf("") }

                    val localDevices = remember { getAttachedUsbDevices() }
                    val discoveredServers = remember { emptyList<DiscoveredServer>() }

                    TvLeanbackScreen(
                        onStartServer = { deviceName ->
                            val device = localDevices.find { it.name == deviceName }
                            if (device != null) {
                                service?.startServer(deviceName, device.vid, device.pid)
                                isRunning = true
                                modeText = "Server — sharing $deviceName"
                            }
                        },
                        onConnectToServer = { host, busId ->
                            val parts = host.split(":")
                            val h = parts[0]
                            val p = if (parts.size > 1)
                                parts[1].toIntOrNull() ?: 3240
                            else
                                3240
                            service?.startClient(h, p, busId)
                            isRunning = true
                            modeText = "Client — connected to $host"
                        },
                        discoveredServers = discoveredServers,
                        localDevices = localDevices,
                        isServiceRunning = isRunning,
                        serviceModeText = modeText,
                    )
                }
            }
        }
    }

    override fun onDestroy() {
        if (serviceBound) {
            unbindService(serviceConnection)
            serviceBound = false
        }
        super.onDestroy()
    }

    /**
     * Enumerate currently attached USB devices via the Android USB Host API.
     */
    private fun getAttachedUsbDevices(): List<LocalUsbDevice> {
        val usbManager = getSystemService(USB_SERVICE) as UsbManager
        return usbManager.deviceList.map { (_, device) ->
            LocalUsbDevice(
                name = device.productName ?: "USB Device ${device.deviceId}",
                vid = device.vendorId,
                pid = device.productId,
            )
        }
    }
}
