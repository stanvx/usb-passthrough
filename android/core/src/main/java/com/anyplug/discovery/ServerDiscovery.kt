package com.anyplug.discovery

import android.content.Context
import android.net.wifi.WifiManager
import android.util.Log
import com.anyplug.model.DiscoveredServer
import com.anyplug.model.RemoteDevice
import java.io.IOException
import java.net.InetAddress
import javax.jmdns.JmDNS
import javax.jmdns.ServiceEvent
import javax.jmdns.ServiceListener
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Layered discovery for AnyPlug servers on the LAN.
 *
 * Three independent sources, all merged into a single `_servers` flow:
 * 1. **mDNS** (`_usbip._tcp.local`) — zero-config when it works.
 * 2. **Last-known good** — probed immediately at [start] from
 *    DataStore so the app feels instant after the first connect.
 * 3. **REST subnet scan** — kicks in after [SCAN_FALLBACK_DELAY_MS]
 *    if mDNS has yielded nothing. Hits `GET /api/status` on every
 *    IP in the phone's /24 subnet with bounded concurrency.
 *
 * Sources are deduped by `server_id` when present (from
 * `/api/status`), or by `host:port` as a fallback for legacy mDNS
 * servers that don't advertise an ID yet.
 *
 * Be aware: holding the WiFi MulticastLock has a battery cost, so
 * [stop] always releases it.
 */
class ServerDiscovery(
    private val appContext: Context,
    private val scope: CoroutineScope = CoroutineScope(Dispatchers.IO + SupervisorJob()),
) {
    private val _servers = MutableStateFlow<List<DiscoveredServer>>(emptyList())
    val servers: StateFlow<List<DiscoveredServer>> = _servers.asStateFlow()

    private var jmdns: JmDNS? = null
    private var multicastLock: WifiManager.MulticastLock? = null
    private var listenerJob: Job? = null
    private val lastKnownStore = LastKnownServerStore(appContext)

    /**
     * Begin browsing the network for USB/IP servers. Safe to call multiple
     * times — only the first call has any effect until [stop] is invoked.
     */
    fun start() {
        if (listenerJob?.isActive == true) return

        listenerJob = scope.launch {
            // Source 2 — last-known good: probe immediately in parallel
            // with mDNS so a returning user sees their server at once.
            launch { probeLastKnown() }

            // Source 1 — mDNS. If it works, great. If not, the scan below
            // covers the gap after a short grace period.
            try {
                withContext(Dispatchers.IO) { openJmDns() }
            } catch (e: Exception) {
                Log.w(TAG, "Failed to start mDNS discovery: ${e.message}")
            }

            // Source 3 — REST subnet scan. Only kicks in if mDNS hasn't
            // produced anything by the time the delay elapses.
            delay(SCAN_FALLBACK_DELAY_MS)
            if (_servers.value.isEmpty()) {
                Log.i(TAG, "mDNS produced nothing after ${SCAN_FALLBACK_DELAY_MS}ms — running REST subnet scan")
                scanSubnet()
            }
        }
    }

    /**
     * Recreate the JmDNS session and re-run all discovery sources.
     * Use when the user pulls to refresh or taps a manual refresh
     * button.
     */
    fun restart() {
        Log.i(TAG, "Restarting discovery")
        stop()
        start()
    }

    /**
     * Persist a server as the user's last-known good choice. Called by
     * the connect path after a successful handshake so the next app
     * start can probe it immediately.
     */
    suspend fun saveLastKnown(endpoint: ServerEndpoint) {
        lastKnownStore.save(endpoint)
    }

    /**
     * Tear down the JmDNS session and release the multicast lock.
     * Safe to call when not started.
     */
    fun stop() {
        listenerJob?.cancel()
        listenerJob = null
        try {
            jmdns?.close()
        } catch (e: IOException) {
            Log.w(TAG, "JmDNS close failed: ${e.message}")
        } finally {
            jmdns = null
        }
        try {
            multicastLock?.takeIf { it.isHeld }?.release()
        } catch (e: RuntimeException) {
            Log.w(TAG, "MulticastLock release failed: ${e.message}")
        } finally {
            multicastLock = null
        }
        _servers.value = emptyList()
    }

    /**
     * Cancel the internal scope. Call when the discovery instance
     * will never be used again.
     */
    fun dispose() {
        stop()
        scope.cancel()
    }

    private suspend fun probeLastKnown() {
        val endpoint = lastKnownStore.load() ?: return
        // The cached endpoint is recent enough to trust — emit it
        // straight to the UI without re-probing. The user will see
        // their last server immediately and can retry by tapping Refresh.
        Log.i(TAG, "Loaded last-known: ${endpoint.host}:${endpoint.apiPort}")
        mergeEndpoint(endpoint)
    }

    private suspend fun scanSubnet() {
        val scanner = SubnetScanner(appContext)
        val endpoints = scanner.scan()
        Log.i(TAG, "Subnet scan found ${endpoints.size} server(s)")
        endpoints.forEach { mergeEndpoint(it) }
    }

    private fun mergeEndpoint(endpoint: ServerEndpoint) {
        // REST/last-known sources don't carry device lists yet. Render
        // a single placeholder so the Connect affordance works; the
        // picker sheet will show it and the user can connect.
        val device = RemoteDevice(name = "USB Device", busId = "1-1", vid = 0, pid = 0)
        val server = DiscoveredServer(
            host = endpoint.host,
            port = endpoint.wirePort,
            devices = listOf(device),
        )
        mergeServer(server)
    }

    private fun openJmDns() {
        val wifi = appContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
        val lock = wifi?.createMulticastLock(MULTICAST_LOCK_TAG)?.also {
            it.setReferenceCounted(false)
            it.acquire()
        }
        multicastLock = lock

        val jmDns = JmDNS.create()
        jmdns = jmDns

        val listener = object : ServiceListener {
            override fun serviceAdded(event: ServiceEvent) {
                // Trigger a resolve by asking JmDNS for the full ServiceInfo.
                try {
                    jmDns.requestServiceInfo(event.type, event.name, true, RESOLVE_TIMEOUT_MS)
                } catch (e: Exception) {
                    Log.w(TAG, "requestServiceInfo failed: ${e.message}")
                }
            }

            override fun serviceRemoved(event: ServiceEvent) {
                // Drop any entry that points at this service name.
                val removedName = event.name
                val current = _servers.value
                val updated = current.filterNot { matchesServiceName(it, removedName) }
                if (updated.size != current.size) {
                    _servers.value = updated
                }
            }

            override fun serviceResolved(event: ServiceEvent) {
                val info = event.info ?: return
                val host = info.inetAddresses.firstOrNull()?.hostAddress
                    ?: info.server.let { if (it.endsWith(".")) it.dropLast(1) else it }
                if (host.isEmpty()) return
                val port = info.port
                val devices = parseDevices(info)
                val server = DiscoveredServer(host = host, port = port, devices = devices)
                mergeServer(server)
            }
        }

        jmDns.addServiceListener(SERVICE_TYPE, listener)
    }

    private fun mergeServer(server: DiscoveredServer) {
        val current = _servers.value
        val byKey = current.associateBy { keyOf(it) }.toMutableMap()
        byKey[keyOf(server)] = server
        _servers.value = byKey.values.sortedBy { it.host }
    }

    private fun keyOf(server: DiscoveredServer): String = "${server.host}:${server.port}"

    /**
     * Best-effort device extraction from a JmDNS ServiceInfo.
     *
     * The AnyPlug Rust server currently advertises device metadata via
     * the service instance name (e.g. "myhost._usbip._tcp.local.") and
     * TXT records. We accept any of:
     *   - TXT key "devices" with comma-separated "vid:pid:bus:name" tuples
     *   - TXT keys vid/pid/bus/name as parallel arrays
     *   - Single device encoded in TXT keys vid/pid/bus/name at top level
     *
     * If nothing is parseable, return a single placeholder device with
     * busId "1-1" so the UI can still render a Connect affordance.
     */
    private fun parseDevices(info: javax.jmdns.ServiceInfo): List<RemoteDevice> {
        val propertyNames: List<String> = java.util.Collections.list(info.getPropertyNames())
        val props: Map<String, ByteArray> = propertyNames
            .associateWith { name -> info.getPropertyBytes(name) ?: ByteArray(0) }

        // (1) devices=vid:pid:bus:name,vid:pid:bus:name
        val devicesStr = props["devices"]?.toString(Charsets.UTF_8)
        if (!devicesStr.isNullOrBlank()) {
            return devicesStr.split(",").mapNotNull { entry ->
                val parts = entry.split(":")
                if (parts.size < 4) return@mapNotNull null
                val vid = parts[0].toIntOrNull(16) ?: return@mapNotNull null
                val pid = parts[1].toIntOrNull(16) ?: return@mapNotNull null
                RemoteDevice(
                    name = parts[3],
                    busId = parts[2],
                    vid = vid,
                    pid = pid,
                )
            }
        }

        // (2) parallel arrays vid[] pid[] bus[] name[]
        val vids = parseUIntList(props["vid"])
        val pids = parseUIntList(props["pid"])
        val buses = parseStringList(props["bus"])
        val deviceNames = parseStringList(props["name"])
        if (vids.isNotEmpty() && vids.size == pids.size) {
            val size = vids.size
            return (0 until size).map { i ->
                RemoteDevice(
                    name = deviceNames.getOrNull(i) ?: "USB Device",
                    busId = buses.getOrNull(i) ?: "${i + 1}-${i + 1}",
                    vid = vids[i],
                    pid = pids[i],
                )
            }
        }

        // (3) single device via top-level vid/pid/bus/name keys
        val vid = props["vid"]?.let { parseUInt(it) }
        val pid = props["pid"]?.let { parseUInt(it) }
        if (vid != null && pid != null) {
            return listOf(
                RemoteDevice(
                    name = props["name"]?.toString(Charsets.UTF_8) ?: "USB Device",
                    busId = props["bus"]?.toString(Charsets.UTF_8) ?: "1-1",
                    vid = vid,
                    pid = pid,
                ),
            )
        }

        // (4) No device metadata — placeholder so the Connect button still works.
        return listOf(RemoteDevice(name = "USB Device", busId = "1-1", vid = 0, pid = 0))
    }

    private fun parseUInt(bytes: ByteArray): Int? {
        val s = String(bytes, Charsets.UTF_8).trim()
        return s.toIntOrNull(16) ?: s.toIntOrNull()
    }

    private fun parseUIntList(bytes: ByteArray?): List<Int> {
        if (bytes == null) return emptyList()
        val s = String(bytes, Charsets.UTF_8)
        return s.split(",", ";").mapNotNull { it.trim().toIntOrNull(16) ?: it.trim().toIntOrNull() }
    }

    private fun parseStringList(bytes: ByteArray?): List<String> {
        if (bytes == null) return emptyList()
        return String(bytes, Charsets.UTF_8)
            .split(",", ";")
            .map { it.trim() }
            .filter { it.isNotEmpty() }
    }

    private fun matchesServiceName(server: DiscoveredServer, serviceName: String): Boolean {
        // The mDNS service name (e.g. "myhost._usbip._tcp.local.") is loosely
        // associated with the host. We treat any server whose host matches
        // the service name prefix as belonging to the same instance.
        val prefix = serviceName.substringBefore("._usbip._tcp.local.", serviceName)
        return server.host == prefix || server.host.startsWith(prefix + ".")
    }

    companion object {
        private const val TAG = "ServerDiscovery"
        private const val SERVICE_TYPE = "_usbip._tcp.local."
        private const val MULTICAST_LOCK_TAG = "anyplug:mdns"
        private const val RESOLVE_TIMEOUT_MS = 1500L
        /** How long to wait after mDNS starts before falling back to
         *  a REST subnet scan. 5 s is the typical mDNS convergence time
         *  on Android; shorter risks false negatives, longer feels slow. */
        private const val SCAN_FALLBACK_DELAY_MS = 5_000L
    }
}
