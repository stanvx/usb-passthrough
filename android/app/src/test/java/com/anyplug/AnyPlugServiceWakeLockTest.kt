package com.anyplug

import android.os.PowerManager
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowPowerManager

/**
 * Tests for WakeLock lifecycle in AnyPlugService.
 *
 * Verifies that:
 * - Transfer wake lock is acquired before URB operations
 * - Transfer wake lock is released after URB operations
 * - Session-level wake lock is acquired on startServer/startClient
 * - Session-level wake lock is released on stop
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28])
class AnyPlugServiceWakeLockTest {

    @Test
    fun `transferWakeLock is initially not acquired`() {
        val service = Robolectric.buildService(AnyPlugService::class.java).get()
        assertFalse("Transfer wake lock should not be held initially", service.isTransferWakeLockHeld())
    }

    @Test
    fun `acquireTransferWakeLock acquires the lock`() {
        val service = Robolectric.buildService(AnyPlugService::class.java).get()
        service.acquireTransferWakeLock()
        assertTrue("Transfer wake lock should be held after acquire", service.isTransferWakeLockHeld())
    }

    @Test
    fun `releaseTransferWakeLock releases the lock`() {
        val service = Robolectric.buildService(AnyPlugService::class.java).get()
        service.acquireTransferWakeLock()
        assertTrue("Precondition: lock should be held", service.isTransferWakeLockHeld())
        service.releaseTransferWakeLock()
        assertFalse("Transfer wake lock should be released", service.isTransferWakeLockHeld())
    }

    @Test
    fun `transferWakeLock is reference counted`() {
        val service = Robolectric.buildService(AnyPlugService::class.java).get()
        service.acquireTransferWakeLock()
        service.acquireTransferWakeLock()
        assertTrue("Lock should be held after double acquire", service.isTransferWakeLockHeld())
        service.releaseTransferWakeLock()
        assertTrue("Lock should still be held after one release (nested)", service.isTransferWakeLockHeld())
        service.releaseTransferWakeLock()
        assertFalse("Lock should be released after second release", service.isTransferWakeLockHeld())
    }

    @Test
    fun `startServer acquires session wake lock`() {
        val service = Robolectric.buildService(AnyPlugService::class.java).create().get()
        // startServer is called but will fail due to no USB device — we only check
        // that the wake lock is acquired before the exception
        try {
            service.startServer("test-device", 0x1234, 0x5678)
        } catch (_: Exception) {
            // Expected: no real USB device attached
        }
        val pm = service.getSystemService(android.content.Context.POWER_SERVICE) as PowerManager
        val wakeLocks = ShadowPowerManager.getWakeLocks()
        val sessionLock = wakeLocks.find { it.tag == "anyplug:wakelock" }
        assertNotNull("Session wake lock should exist", sessionLock)
    }

    @Test
    fun `stop releases session wake lock`() {
        val service = Robolectric.buildService(AnyPlugService::class.java).get()
        val pm = service.getSystemService(android.content.Context.POWER_SERVICE) as PowerManager
        val wl = ShadowPowerManager.newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK,
            "anyplug:wakelock"
        )
        wl.acquire()
        service.stop()
        assertFalse("Session wake lock should be released after stop", wl.isHeld)
    }
}
