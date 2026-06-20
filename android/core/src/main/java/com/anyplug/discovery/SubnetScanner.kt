package com.anyplug.discovery

import android.content.Context
import android.net.wifi.WifiManager
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withContext
import java.net.InetAddress

/**
 * Scan the /24 subnet the phone is on, hitting each candidate IP with
 * a short REST probe. Returns the AnyPlug servers it finds.
 *
 * Why /24 only:
 * - 192.168.x.0/16 would mean 65 536 hosts — slow, battery-heavy,
 *   and privacy-hostile (probes every host on the LAN).
 * - 192.168.x.0/24 is what most consumer routers use; the phone's
 *   DHCP lease is always in this range, and the gateway that hands
 *   out leases is also in it.
 *
 * Concurrency: 64 parallel probes. With 500 ms per probe, scanning
 * the full /24 takes ~2 s in the worst case.
 */
internal class SubnetScanner(
    private val appContext: Context,
    private val apiPort: Int = 3241,
    private val parallelProbes: Int = 64,
) {
    private companion object {
        const val TAG = "SubnetScanner"
    }

    suspend fun scan(): List<ServerEndpoint> = withContext(Dispatchers.IO) {
        val gateway = gatewayAddress()
        if (gateway == null) {
            Log.w(TAG, "scan: no gateway address; aborting")
            return@withContext emptyList()
        }
        val prefixLength = subnetPrefixLength() ?: 24
        if (prefixLength != 24) {
            Log.w(TAG, "scan: prefix length $prefixLength not /24; out of scope for v1")
            return@withContext emptyList()
        }
        Log.i(TAG, "scan: gateway=${gateway.hostAddress} prefix=$prefixLength")
        val candidates = expandHosts(gateway)
        candidates.chunked(parallelProbes).flatMap { batch ->
            coroutineScope {
                batch.mapNotNull { ip ->
                    val hostStr = ip.hostAddress ?: return@mapNotNull null
                    async { RestProbe.probe(hostStr, apiPort) }
                }.awaitAll()
            }
        }.filterNotNull()
    }

    private fun gatewayAddress(): InetAddress? {
        // Try ConnectivityManager first. The active network's link
        // properties expose routes including the default route's
        // gateway. This works on Android 10+ where DhcpInfo is
        // deprecated.
        val cm = appContext.getSystemService(Context.CONNECTIVITY_SERVICE)
            as? android.net.ConnectivityManager
        val allNetworks = cm?.allNetworks.orEmpty()
        for (n in allNetworks) {
            // Only consider networks with WiFi transport — cellular
            // networks (rmnet_data) are CGNAT and not on the home LAN.
            val caps = cm?.getNetworkCapabilities(n)
            if (caps?.hasTransport(android.net.NetworkCapabilities.TRANSPORT_WIFI) != true) continue
            val linkProps = cm?.getLinkProperties(n) ?: continue
            val gateway = linkProps.routes
                .mapNotNull { route ->
                    val gw = route.gateway as? InetAddress ?: return@mapNotNull null
                    if (route.isDefaultRoute && gw.hostAddress?.contains('.') == true) {
                        gw.hostAddress
                    } else {
                        null
                    }
                }
                .firstOrNull()
            if (gateway != null) {
                return runCatching { InetAddress.getByName(gateway) }.getOrNull()
            }
        }

        // Last-resort: parse /proc/net/route. The kernel exposes the
        // default route's gateway as the second column of the row whose
        // destination is 00000000. Hex little-endian.
        try {
            java.io.File("/proc/net/route").useLines { lines ->
                lines.drop(1).forEach { line ->
                    val cols = line.split('\t')
                    if (cols.size >= 3 && cols[1] == "00000000") {
                        val gwHex = cols[2]
                        if (gwHex.isNotEmpty() && gwHex != "00000000") {
                            val bytes = byteArrayOf(
                                gwHex.substring(6, 8).toInt(16).toByte(),
                                gwHex.substring(4, 6).toInt(16).toByte(),
                                gwHex.substring(2, 4).toInt(16).toByte(),
                                gwHex.substring(0, 2).toInt(16).toByte(),
                            )
                            val addr = runCatching {
                                InetAddress.getByAddress(bytes).hostAddress
                            }.getOrNull()
                            if (addr != null) {
                                Log.i(TAG, "gateway from /proc/net/route = $addr")
                                return runCatching { InetAddress.getByName(addr) }.getOrNull()
                            }
                        }
                    }
                }
            }
        } catch (_: Exception) {
            // /proc/net/route is world-readable on Android but can fail
            // in restricted profiles. Silently ignore.
        }

        Log.w(TAG, "no usable gateway found; aborting scan")
        return null
    }

    private fun subnetPrefixLength(): Int? {
        val wifi = appContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            ?: return null
        @Suppress("DEPRECATION")
        val dhcp = wifi.dhcpInfo ?: return null
        // netmask is the AND of all 4 bytes; count the leading 1 bits.
        val mask = dhcp.netmask
        if (mask == 0) return null
        // Avoid sign-extension: mask.inv() on Int is fine but the literal
        // 0xffffffff is read as Int (negative) — mask with `and 0xffffffff`
        // to keep the bit-pattern intact, then count leading zeros.
        val inverted = mask.inv() and 0xffffffff.toInt()
        return 32 - Integer.numberOfLeadingZeros(inverted)
    }

    private fun expandHosts(gateway: InetAddress): List<InetAddress> {
        val bytes = gateway.address
        // Probe .1 through .254 (skipping the gateway itself is unnecessary —
        // many home routers run an embedded AnyPlug server, and the gateway
        // and the phone will both answer their own probes harmlessly).
        return (1..254).mapNotNull { host ->
            runCatching {
                InetAddress.getByAddress(
                    byteArrayOf(bytes[0], bytes[1], bytes[2], host.toByte())
                )
            }.getOrNull()
        }
    }
}

private fun Int.toByteArray4(): ByteArray = byteArrayOf(
    ((this ushr 24) and 0xff).toByte(),
    ((this ushr 16) and 0xff).toByte(),
    ((this ushr 8) and 0xff).toByte(),
    (this and 0xff).toByte(),
)