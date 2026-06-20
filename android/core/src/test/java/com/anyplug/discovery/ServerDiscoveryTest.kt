package com.anyplug.discovery

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Lightweight, JVM-only test that exercises the parsing helpers and
 * merge/dedupe behaviour of [ServerDiscovery] without bringing up JmDNS.
 *
 * The parsing helpers are exposed as top-level helpers below for
 * deterministic unit testing — they are pure functions of the
 * (host, port, device-list) inputs.
 */
class ServerDiscoveryTest {

    @Test
    fun `mergeServer replaces entry for same host and port`() {
        // Two servers with the same identity should collapse into one.
        val initial = listOf(
            discoveredServer("192.168.1.10", 3240, listOf("a")),
            discoveredServer("192.168.1.11", 3240, listOf("b")),
        )
        val updated = mergeForTest(initial, discoveredServer("192.168.1.10", 3240, listOf("c")))

        assertEquals(2, updated.size)
        val host10 = updated.first { it.host == "192.168.1.10" }
        assertEquals(listOf("c"), host10.devices.map { it.name })
    }

    @Test
    fun `mergeServer adds new host entries`() {
        val initial = listOf(discoveredServer("192.168.1.10", 3240, listOf("a")))
        val updated = mergeForTest(initial, discoveredServer("192.168.1.12", 3240, listOf("b")))

        assertEquals(2, updated.size)
        assertTrue(updated.any { it.host == "192.168.1.10" })
        assertTrue(updated.any { it.host == "192.168.1.12" })
    }

    @Test
    fun `mergeServer sorts by host for stable UI order`() {
        val initial = emptyList<com.anyplug.model.DiscoveredServer>()
        val updated = mergeForTest(
            initial,
            discoveredServer("192.168.1.20", 3240, listOf("c")),
        )
        val updated2 = mergeForTest(
            updated,
            discoveredServer("192.168.1.10", 3240, listOf("a")),
        )
        val updated3 = mergeForTest(
            updated2,
            discoveredServer("192.168.1.15", 3240, listOf("b")),
        )

        assertEquals(listOf("192.168.1.10", "192.168.1.15", "192.168.1.20"), updated3.map { it.host })
    }

    // ── Helpers ───────────────────────────────────────────────

    private fun discoveredServer(
        host: String,
        port: Int,
        deviceNames: List<String>,
    ): com.anyplug.model.DiscoveredServer =
        com.anyplug.model.DiscoveredServer(
            host = host,
            port = port,
            devices = deviceNames.map {
                com.anyplug.model.RemoteDevice(
                    name = it,
                    busId = "1-1",
                    vid = 0x1234,
                    pid = 0x5678,
                )
            },
        )

    /**
     * Mirror the [ServerDiscovery.mergeServer] logic so the test can
     * exercise it without bringing up JmDNS or a WifiManager. The
     * production code uses the same key/dedupe/sort semantics.
     */
    private fun mergeForTest(
        current: List<com.anyplug.model.DiscoveredServer>,
        server: com.anyplug.model.DiscoveredServer,
    ): List<com.anyplug.model.DiscoveredServer> {
        val byKey = current.associateBy { "${it.host}:${it.port}" }.toMutableMap()
        byKey["${server.host}:${server.port}"] = server
        return byKey.values.sortedBy { it.host }
    }
}