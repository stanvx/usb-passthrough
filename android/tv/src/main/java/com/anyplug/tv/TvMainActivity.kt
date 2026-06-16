package com.anyplug.tv

import android.content.BroadcastReceiver
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
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import com.anyplug.AnyPlugService
import com.anyplug.UsbPermissionHandler
import com.anyplug.attachedDevices
import com.anyplug.bridge.RustBridge
import com.anyplug.findDevice
import com.anyplug.model.DiscoveredServer
import com.anyplug.model.LocalUsbDevice
import com.anyplug.parseHostPort
import com.anyplug.registerReceiverSafely
import com.anyplug.tv.theme.TvTheme
import com.anyplug.tv.ui.TvLeanbackScreen
import com.anyplug.usbManager

/**
 * Android TV launcher activity with Leanback-optimised UI.
 *
 * Uses the dark [TvTheme] with enlarged typography and D-pad
 * focus management for TV remote operation.
 *
 * Handles USB hotplug events via [onNewIntent] and a runtime
 * broadcast receiver for [UsbManager.ACTION_USB_DEVICE_DETACHED]
 * so the device list stays current without restarting the TV app.
 */
class TvMainActivity : ComponentActivity() {

    private var service: AnyPlugService? = null
    private var permissionHandler: UsbPermissionHandler? = null

    // Reactive USB device list — updated from hotplug callbacks
    private val localDevices = mutableStateOf(emptyList<LocalUsbDevice>())

    // Manifest only covers ATTACHED; this catches DETACHED at runtime
    private val detachReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (UsbManager.ACTION_USB_DEVICE_DETACHED == intent.action) {
                localDevices.value = usbManager.attachedDevices()
            }
        }
    }

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, binder: IBinder?) {
            service = (binder as AnyPlugService.LocalBinder).getService()
        }

        override fun onServiceDisconnected(name: ComponentName?) {
            service = null
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        enableEdgeToEdge()
        RustBridge.init()

        val intent = Intent(this, AnyPlugService::class.java)
        bindService(intent, serviceConnection, Context.BIND_AUTO_CREATE)

        registerReceiverSafely(detachReceiver, UsbManager.ACTION_USB_DEVICE_DETACHED)
        permissionHandler = UsbPermissionHandler(this)

        localDevices.value = usbManager.attachedDevices()

        setContent {
            TvTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    TvScreenContent()
                }
            }
        }
    }

    /**
     * Handles USB_DEVICE_ATTACHED / DETACHED intents delivered when
     * the activity is already running (singleTop mode).
     */
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        if (intent.action == UsbManager.ACTION_USB_DEVICE_ATTACHED ||
            intent.action == UsbManager.ACTION_USB_DEVICE_DETACHED) {
            localDevices.value = usbManager.attachedDevices()
        }
    }

    override fun onDestroy() {
        permissionHandler?.unregister()
        try {
            unregisterReceiver(detachReceiver)
        } catch (_: IllegalArgumentException) { /* already unregistered */ }
        if (service != null) {
            unbindService(serviceConnection)
        }
        super.onDestroy()
    }

    // ─── Composable tree ───────────────────────────────────────

    @Composable
    private fun TvScreenContent() {
        val discoveredServers = remember { emptyList<DiscoveredServer>() }
        val devices by localDevices
        val isRunning = service?.isRunning() ?: false
        val modeText = service?.getModeText() ?: ""
        val sharedDeviceName = service?.getSharedDeviceName() ?: ""

        TvLeanbackScreen(
            onStartServer = { deviceName ->
                val device = devices.find { it.name == deviceName }
                if (device != null) {
                    val usbDevice = usbManager.findDevice(device)
                    if (usbDevice != null && !usbManager.hasPermission(usbDevice)) {
                        permissionHandler?.requestPermission(usbDevice) { granted ->
                            if (granted) {
                                service?.startServer(deviceName, device.vid, device.pid)
                            }
                        }
                    } else {
                        service?.startServer(deviceName, device.vid, device.pid)
                    }
                }
            },
            onStopService = { service?.stop() },
            onConnectToServer = { host, busId ->
                val (h, p) = parseHostPort(host)
                service?.startClient(h, p, busId)
            },
            discoveredServers = discoveredServers,
            localDevices = devices,
            isServiceRunning = isRunning,
            serviceModeText = modeText,
            sharedDeviceName = sharedDeviceName,
        )
    }
}
