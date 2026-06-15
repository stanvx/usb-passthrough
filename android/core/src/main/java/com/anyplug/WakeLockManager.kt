package com.anyplug

/**
 * Interface for managing power wake lock during USB/IP URB transfers.
 *
 * Implemented by AnyPlugService to provide CPU wake lock
 * scoped to individual URB transfers rather than the whole session.
 * This allows the CPU to sleep between transfers, saving power
 * on TV and mobile devices.
 */
interface WakeLockManager {
    /** Acquire wake lock before a URB transfer. */
    fun acquireTransferWakeLock()

    /** Release wake lock after a URB transfer completes. */
    fun releaseTransferWakeLock()
}
