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

/**
 * @property deviceClass USB class code (0x08 = mass storage).
 *   Used to show safety warnings before sharing storage devices.
 */
data class LocalUsbDevice(
    val name: String,
    val vid: Int,
    val pid: Int,
    val deviceClass: Int = 0,
) {
    companion object {
        /** USB mass storage device class (06 = imaging, 08 = mass storage). */
        const val CLASS_MASS_STORAGE = 0x08
    }

    /** True when this is a mass storage device (flash drive, hard drive, etc.). */
    val isMassStorage: Boolean get() = deviceClass == CLASS_MASS_STORAGE
}
