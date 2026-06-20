package com.anyplug

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.os.Binder
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.lifecycle.LifecycleService
import com.anyplug.client.UsbIpClient
import com.anyplug.discovery.ServerDiscovery
import com.anyplug.model.DiscoveredServer
import com.anyplug.server.UsbIpServer
import com.anyplug.server.UsbIpServer.UsbDeviceFilter
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Foreground service that keeps the USB/IP connection alive.
 *
 * Runs either as server (exporting a local USB device) or client
 * (importing a remote USB device). The service holds a wake lock
 * to prevent the CPU from sleeping during active transfers.
 */
class AnyPlugService : LifecycleService(), WakeLockManager {

    private val binder = LocalBinder()
    private val serviceScope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    /** One-shot error events propagated to the UI layer. */
    private val _errors = MutableSharedFlow<String>(extraBufferCapacity = 1)
    val errors: SharedFlow<String> = _errors.asSharedFlow()

    /** Reactive state observers — UI collects this to redraw when mode changes. */
    private val _state = MutableStateFlow(Mode.IDLE)
    val state: StateFlow<Mode> = _state.asStateFlow()

    /**
     * mDNS-based LAN discovery for USB/IP servers.
     * Created in [onCreate] and started/stopped via [startDiscovery] / [stopDiscovery].
     */
    private var serverDiscovery: ServerDiscovery? = null

    /**
     * Stream of currently-discovered servers. UI collects this to render
     * the LAN client panel. Empty when discovery is not running.
     */
    val discoveredServers: StateFlow<List<DiscoveredServer>>
        get() = serverDiscovery?.servers
            ?: MutableStateFlow(emptyList())

    private var wakeLock: PowerManager.WakeLock? = null
    private var transferWakeLock: PowerManager.WakeLock? = null
    private var serverRunner: UsbIpServer? = null
    private var clientRunner: UsbIpClient? = null

    enum class Mode { SERVER, CLIENT, IDLE }
    var currentMode: Mode = Mode.IDLE
        private set

    companion object {
        private const val CHANNEL_ID = "anyplug_channel"
        private const val WAKE_LOCK_TAG = "anyplug:wakelock"
        private const val TRANSFER_WAKE_LOCK_TAG = "anyplug:transferwake"
        private const val SESSION_WAKE_LOCK_TIMEOUT_MS = 60L * 60L * 1000L
    }

    inner class LocalBinder : Binder() {
        fun getService(): AnyPlugService = this@AnyPlugService
    }

    override fun onBind(intent: Intent): IBinder {
        super.onBind(intent)
        return binder
    }

    override fun onCreate() {
        super.onCreate()

        // Create the notification channel up-front so startForeground()
        // never throws IllegalArgumentException on API 26+.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "AnyPlug Service",
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = "Keeps the USB/IP connection alive"
                setShowBadge(false)
            }
            val nm = getSystemService(NotificationManager::class.java)
            nm.createNotificationChannel(channel)
        }

        // Acquire partial wake lock (keep CPU on, screen can sleep).
        // Safety timeout ensures the lock can never be held forever if
        // the service is killed without releasing.
        val pm = getSystemService(POWER_SERVICE) as PowerManager
        wakeLock = pm.newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK,
            WAKE_LOCK_TAG,
        )
        wakeLock?.setReferenceCounted(false)

        // Transfer wake lock — finer granularity, acquired per-URB
        transferWakeLock = pm.newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK,
            TRANSFER_WAKE_LOCK_TAG,
        )
        transferWakeLock?.setReferenceCounted(true)

        // mDNS LAN discovery — starts lazily via [startDiscovery] so the
        // multicast lock is not held before the user opens the client panel.
        serverDiscovery = ServerDiscovery(applicationContext, serviceScope)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)

        val notificationIntent =
            packageManager.getLaunchIntentForPackage(packageName)
                ?: Intent()
        val pendingIntent = PendingIntent.getActivity(
            this,
            0,
            notificationIntent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

        val notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("AnyPlug")
            .setContentText("Service running")
            .setSmallIcon(android.R.drawable.ic_menu_share)
            .setContentIntent(pendingIntent)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()

        startForeground(1001, notification)

        return START_STICKY
    }

    override fun onDestroy() {
        serviceScope.cancel()
        serverDiscovery?.dispose()
        serverDiscovery = null
        if (wakeLock?.isHeld == true) wakeLock?.release()
        if (transferWakeLock?.isHeld == true) transferWakeLock?.release()
        serverRunner?.stop()
        clientRunner?.stop()
        super.onDestroy()
    }

    // ─── Public API ──────────────────────────────────────────

    /**
     * Begin mDNS discovery for USB/IP servers on the LAN. Idempotent.
     * Discovery is opt-in so the multicast lock is not held until the
     * user actually opens the client panel.
     */
    fun startDiscovery() {
        serverDiscovery?.start()
    }

    /**
     * Stop mDNS discovery and release the multicast lock. Idempotent.
     */
    fun stopDiscovery() {
        serverDiscovery?.stop()
    }

    // ─── State queries ─────────────────────────────────────────

    /**
     * The name of the device currently being shared (server mode) or
     * the host being connected to (client mode). Empty when idle.
     */
    private var sharedDeviceName: String = ""

    /**
     * True when the service is actively sharing or importing a device.
     */
    fun isRunning(): Boolean = currentMode != Mode.IDLE

    /**
     * Returns a human-readable mode description for the StatusCard.
     * Example: "Server — sharing USB Drive" or "Client — connected to 192.168.1.5"
     */
    fun getModeText(): String = when (currentMode) {
        Mode.SERVER -> "Server — sharing $sharedDeviceName"
        Mode.CLIENT -> "Client — connected"
        Mode.IDLE -> ""
    }

    /** The name of the device currently being exported, or empty when idle. */
    fun getSharedDeviceName(): String = sharedDeviceName

    /**
     * Start exporting a USB device. The service becomes a USB/IP server.
     *
     * Errors are emitted via [errors] so the UI can show meaningful feedback.
     */
    fun startServer(deviceName: String, vid: Int, pid: Int) {
        // Promote from bound-only to started+foreground so the system
        // doesn't kill the service while a USB/IP connection is live.
        startServiceCompat()

        currentMode = Mode.SERVER
        _state.value = Mode.SERVER
        sharedDeviceName = deviceName
        android.util.Log.i("AnyPlugService", "startServer: $deviceName (vid=$vid pid=$pid)")
        if (wakeLock?.isHeld == false) {
            wakeLock?.acquire(SESSION_WAKE_LOCK_TIMEOUT_MS)
        }

        serverRunner = UsbIpServer(
            context = this,
            deviceFilter = UsbDeviceFilter(vid, pid),
            wakeLockManager = this
        )
        serviceScope.launch {
            try {
                serverRunner?.start()
            } catch (e: Exception) {
                // SocketException on accept() means we called stop() — not an error.
                if (e is java.net.SocketException && currentMode == Mode.IDLE) {
                    android.util.Log.i("AnyPlugService", "Server accept() interrupted by stop()")
                } else {
                    android.util.Log.e("AnyPlugService", "Server failed to start", e)
                    _errors.tryEmit(e.message ?: "Server failed to start: ${e.javaClass.simpleName}")
                }
                currentMode = Mode.IDLE
                _state.value = Mode.IDLE
                sharedDeviceName = ""
                serverRunner?.stop()
                serverRunner = null
                if (wakeLock?.isHeld == true) wakeLock?.release()
            }
        }
    }

    /**
     * Start importing a USB device from a remote server.
     *
     * Errors are emitted via [errors] so the UI can show meaningful feedback.
     */
    fun startClient(serverHost: String, serverPort: Int, busId: String) {
        if (serverHost.isBlank() || serverPort !in 1..65535) {
            _errors.tryEmit("Invalid server address: $serverHost:$serverPort")
            return
        }
        startServiceCompat()

        currentMode = Mode.CLIENT
        _state.value = Mode.CLIENT
        sharedDeviceName = "$serverHost:$serverPort"
        if (wakeLock?.isHeld == false) {
            wakeLock?.acquire(SESSION_WAKE_LOCK_TIMEOUT_MS)
        }

        clientRunner = UsbIpClient(
            serverHost = serverHost,
            serverPort = serverPort,
            busId = busId,
            wakeLockManager = this
        )
        serviceScope.launch {
            try {
                clientRunner?.start()
            } catch (e: Exception) {
                if (e is java.net.SocketException && currentMode == Mode.IDLE) {
                    android.util.Log.i("AnyPlugService", "Client interrupted by stop()")
                } else {
                    _errors.tryEmit(e.message ?: "Client failed to connect")
                }
                currentMode = Mode.IDLE
                _state.value = Mode.IDLE
                sharedDeviceName = ""
                clientRunner?.stop()
                clientRunner = null
                if (wakeLock?.isHeld == true) wakeLock?.release()
            }
        }
    }

    private fun startServiceCompat() {
        val intent = Intent(this, AnyPlugService::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            startService(intent)
        }
    }

    /**
     * Stop all USB/IP activity.
     */
    fun stop() {
        // Mark as IDLE FIRST so the server's accept() failure is
        // recognized as a deliberate stop rather than a server crash.
        currentMode = Mode.IDLE
        _state.value = Mode.IDLE
        sharedDeviceName = ""
        serverRunner?.stop()
        clientRunner?.stop()
        if (wakeLock?.isHeld == true) wakeLock?.release()
        if (transferWakeLock?.isHeld == true) transferWakeLock?.release()
        stopSelf()
    }

    /**
     * Acquire a reference-counted wake lock for an individual URB transfer.
     *
     * Reference-counted — pair each acquire with a release.
     */
    override fun acquireTransferWakeLock() {
        transferWakeLock?.acquire()
    }

    /**
     * Release the transfer wake lock after a URB operation completes.
     */
    override fun releaseTransferWakeLock() {
        transferWakeLock?.release()
    }

    /**
     * Check whether the transfer wake lock is currently held.
     * Used for testing and diagnostics.
     */
    fun isTransferWakeLockHeld(): Boolean {
        return transferWakeLock?.isHeld == true
    }
}
