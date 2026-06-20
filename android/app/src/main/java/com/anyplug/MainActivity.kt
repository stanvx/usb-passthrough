package com.anyplug

import android.Manifest
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.hardware.usb.UsbManager
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import android.widget.Toast
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import com.anyplug.bridge.RustBridge
import com.anyplug.findDevice
import com.anyplug.model.DiscoveredServer
import com.anyplug.model.LocalUsbDevice
import com.anyplug.parseHostPort
import com.anyplug.theme.AnyPlugTheme
import com.anyplug.ui.MainScreen
import com.anyplug.usbManager

/**
 * Phone / tablet launcher activity for AnyPlug.
 *
 * Binds to [AnyPlugService] and renders the Compose UI with
 * an M3 Expressive theme powered by [AnyPlugTheme].
 *
 * Handles USB hotplug events via [onNewIntent] and a runtime
 * broadcast receiver for [UsbManager.ACTION_USB_DEVICE_DETACHED]
 * so the device list stays current without restarting the app.
 *
 * @see AnyPlugService
 * @see MainScreen
 */
class MainActivity : ComponentActivity() {

    private var service: AnyPlugService? = null
    private var permissionHandler: UsbPermissionHandler? = null

    // Reactive USB device list — updated from hotplug callbacks
    private val localDevices = mutableStateOf(emptyList<LocalUsbDevice>())

    // Reactive service state — updated by collecting the service's state flow
    private val serviceMode = mutableStateOf(AnyPlugService.Mode.IDLE)
    private val sharedDeviceNameState = mutableStateOf("")

    // mDNS-discovered servers — pushed from the service's flow
    private val discoveredServersState = mutableStateOf(emptyList<DiscoveredServer>())

    // Triggers recomposition when the service binds/unbinds
    private val serviceConnected = mutableStateOf(false)

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
            serviceConnected.value = true
            // Begin LAN discovery as soon as the service is bound. The
            // service holds the multicast lock and will release it in
            // onDestroy or when stopDiscovery() is called.
            service?.startDiscovery()
        }

        override fun onServiceDisconnected(name: ComponentName?) {
            service = null
            serviceConnected.value = false
        }
    }

    // Observe mode changes from the service when re-attaching to it
    private val modeCollector: suspend (AnyPlugService.Mode) -> Unit = { mode ->
        serviceMode.value = mode
        if (mode == AnyPlugService.Mode.IDLE) {
            sharedDeviceNameState.value = ""
        } else {
            sharedDeviceNameState.value = service?.getSharedDeviceName() ?: ""
        }
    }

    companion object {
        private const val REQ_POST_NOTIFICATIONS = 1001
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

        requestNotificationPermissionIfNeeded()

        setContent {
            AnyPlugTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    MainScreenContent()
                }
            }
        }
    }

    private fun requestNotificationPermissionIfNeeded() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val granted = ContextCompat.checkSelfPermission(
            this,
            Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
        if (!granted) {
            ActivityCompat.requestPermissions(
                this,
                arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                REQ_POST_NOTIFICATIONS,
            )
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

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == REQ_POST_NOTIFICATIONS) {
            // User can deny — service still runs, but notification is hidden.
            // We don't surface this as an error to keep the flow simple.
        }
    }

    override fun onDestroy() {
        permissionHandler?.unregister()
        try {
            unregisterReceiver(detachReceiver)
        } catch (_: IllegalArgumentException) { /* already unregistered */ }
        // Stop LAN discovery when the activity goes away so the multicast
        // lock is released. The service will re-start it on next bind.
        service?.stopDiscovery()
        if (service != null) {
            unbindService(serviceConnection)
        }
        super.onDestroy()
    }

    // ─── Composable tree ───────────────────────────────────────

    @Composable
    private fun MainScreenContent() {
        val context = LocalContext.current
        val discoveredServers by discoveredServersState
        val devices by localDevices
        val connected by serviceConnected
        // Touch connected to force recomposition when service binds/unbinds
        @Suppress("UNUSED_VARIABLE")
        val unused = connected
        val mode by serviceMode
        val sharedName by sharedDeviceNameState

        val isRunning = mode != AnyPlugService.Mode.IDLE
        val modeText = when (mode) {
            AnyPlugService.Mode.SERVER -> "Server — sharing $sharedName"
            AnyPlugService.Mode.CLIENT -> "Client — connected"
            AnyPlugService.Mode.IDLE -> ""
        }

        // Collect service-level errors and surface them to the user
        LaunchedEffect(service) {
            service?.errors?.collect { msg ->
                Toast.makeText(context, msg, Toast.LENGTH_LONG).show()
            }
        }

        // Collect service mode changes — re-runs when serviceConnected flips
        LaunchedEffect(connected, service) {
            service?.let { svc ->
                svc.state.collect(modeCollector)
            }
        }

        // Mirror mDNS-discovered servers into a Compose state.
        LaunchedEffect(connected, service) {
            service?.let { svc ->
                svc.discoveredServers.collect { servers ->
                    discoveredServersState.value = servers
                }
            }
        }

        MainScreen(
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
            onRefreshDiscovery = { service?.restartDiscovery() },
            discoveredServers = discoveredServers,
            localDevices = devices,
            isServiceRunning = isRunning,
            serviceModeText = modeText,
            sharedDeviceName = sharedName,
        )
    }
}
