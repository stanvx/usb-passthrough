package com.anyplug

import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.os.Binder
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.lifecycle.LifecycleService
import com.anyplug.client.UsbIpClient
import com.anyplug.server.UsbDeviceFilter
import com.anyplug.server.UsbIpServer
import kotlinx.coroutines.*

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

    private var wakeLock: PowerManager.WakeLock? = null
    private var transferWakeLock: PowerManager.WakeLock? = null
    private var serverRunner: UsbIpServer? = null
    private var clientRunner: UsbIpClient? = null

    enum class Mode { SERVER, CLIENT, IDLE }
    var currentMode: Mode = Mode.IDLE
        private set

    inner class LocalBinder : Binder() {
        fun getService(): AnyPlugService = this@AnyPlugService
    }

    override fun onBind(intent: Intent): IBinder {
        super.onBind(intent)
        return binder
    }

    override fun onCreate() {
        super.onCreate()

        // Acquire partial wake lock (keep CPU on, screen can sleep)
        val pm = getSystemService(POWER_SERVICE) as PowerManager
        wakeLock = pm.newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK,
            "anyplug:wakelock"
        )
        wakeLock?.setReferenceCounted(false)

        // Transfer wake lock — finer granularity, acquired per-URB
        transferWakeLock = pm.newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK,
            "anyplug:transferwake"
        )
        transferWakeLock?.setReferenceCounted(true)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)

        val notificationIntent = Intent(this, MainActivity::class.java)
        val pendingIntent = PendingIntent.getActivity(
            this, 0, notificationIntent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

        val notification = NotificationCompat.Builder(this, "anyplug_channel")
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
        wakeLock?.release()
        transferWakeLock?.release()
        serverRunner?.stop()
        clientRunner?.stop()
        super.onDestroy()
    }

    // ─── Public API ──────────────────────────────────────────

    /**
     * Start exporting a USB device. The service becomes a USB/IP server.
     */
    fun startServer(deviceName: String, vid: Int, pid: Int) {
        currentMode = Mode.SERVER
        wakeLock?.acquire()

        serverRunner = UsbIpServer(
            context = this,
            deviceFilter = UsbDeviceFilter(vid, pid),
            wakeLockManager = this
        )
        serviceScope.launch {
            serverRunner?.start()
        }
    }

    /**
     * Start importing a USB device from a remote server.
     */
    fun startClient(serverHost: String, serverPort: Int, busId: String) {
        currentMode = Mode.CLIENT
        wakeLock?.acquire()

        clientRunner = UsbIpClient(
            serverHost = serverHost,
            serverPort = serverPort,
            busId = busId,
            wakeLockManager = this
        )
        serviceScope.launch {
            clientRunner?.start()
        }
    }

    /**
     * Stop all USB/IP activity.
     */
    fun stop() {
        currentMode = Mode.IDLE
        serverRunner?.stop()
        clientRunner?.stop()
        wakeLock?.release()
        transferWakeLock?.release()
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
