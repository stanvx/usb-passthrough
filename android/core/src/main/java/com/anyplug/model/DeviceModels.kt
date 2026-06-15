package com.anyplug.model

/**
 * Shared device models used by both phone (Compose) and TV (Leanback) UIs.
 * Extracted from MainScreen.kt to avoid module dependency on :app.
 */
data class DiscoveredServer(
    val host: String,
    val port: Int,
    val devices: List<RemoteDevice>
)

data class RemoteDevice(
    val name: String,
    val busId: String,
    val vid: Int,
    val pid: Int
)

data class LocalUsbDevice(
    val name: String,
    val vid: Int,
    val pid: Int
)
