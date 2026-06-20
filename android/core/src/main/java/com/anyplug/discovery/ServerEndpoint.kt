package com.anyplug.discovery

/**
 * Identity of an AnyPlug server, as reported by `GET /api/status` or
 * observed via mDNS. Used by both the discovery layer (to merge results
 * from multiple sources) and the last-known-good cache.
 *
 * Two servers are "the same" if their [serverId] matches. When the
 * server hasn't yet reported an ID (e.g. legacy mDNS without the
 * new field), dedupe falls back to host:port.
 */
data class ServerEndpoint(
    val host: String,
    val apiPort: Int,
    val wirePort: Int,
    val serverId: String? = null,
    val serverName: String? = null,
    val source: DiscoverySource = DiscoverySource.UNKNOWN,
) {
    val cacheKey: String
        get() = serverId ?: "$host:$apiPort"
}

enum class DiscoverySource {
    MDNS, REST, LAST_KNOWN, UNKNOWN;

    /** Short label for UI badges. */
    val label: String
        get() = when (this) {
            MDNS -> "mDNS"
            REST -> "LAN"
            LAST_KNOWN -> "Recent"
            UNKNOWN -> ""
        }
}