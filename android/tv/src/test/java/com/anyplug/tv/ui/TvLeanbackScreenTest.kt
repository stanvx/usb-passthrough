package com.anyplug.tv.ui

import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.anyplug.model.DiscoveredServer
import com.anyplug.model.LocalUsbDevice
import com.anyplug.model.RemoteDevice
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Tests for TV-optimised Leanback UI components.
 *
 * Verifies:
 * - Section headers render with correct text
 * - Device cards display name and VID:PID
 * - Empty state messages appear when no devices found
 * - Service status indicator shows correct state
 * - Stop button appears when running
 * - Manual connection input renders correctly
 */
@RunWith(AndroidJUnit4::class)
class TvLeanbackScreenTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    @Test
    fun tvSectionHeader_displaysTitle() {
        composeTestRule.setContent {
            TvSectionHeader("Test Devices")
        }
        composeTestRule.onNodeWithText("Test Devices").assertExists()
    }

    @Test
    fun tvDeviceCard_showsTitleAndSubtitle() {
        composeTestRule.setContent {
            TvDeviceCard(
                title = "USB Device",
                subtitle = "1234:5678",
                actionLabel = "Share",
                onAction = {},
            )
        }
        composeTestRule.onNodeWithText("USB Device").assertExists()
        composeTestRule.onNodeWithText("1234:5678").assertExists()
        composeTestRule.onNodeWithText("Share").assertExists()
    }

    @Test
    fun tvDeviceCard_showsStopSharingWhenDestructive() {
        composeTestRule.setContent {
            TvDeviceCard(
                title = "USB Device",
                subtitle = "1234:5678",
                actionLabel = "Stop Sharing",
                onAction = {},
                isDestructive = true,
            )
        }
        composeTestRule.onNodeWithText("Stop Sharing").assertExists()
    }

    @Test
    fun tvEmptyState_showsProvidedMessage() {
        composeTestRule.setContent {
            TvEmptyState("No devices found. Connect a USB device.")
        }
        composeTestRule.onNodeWithText("No devices found. Connect a USB device.").assertExists()
    }

    @Test
    fun tvButton_displaysLabel() {
        composeTestRule.setContent {
            TvButton(label = "Connect", onClick = {})
        }
        composeTestRule.onNodeWithText("Connect").assertExists()
    }

    @Test
    fun tvButton_displaysDestructiveLabel() {
        composeTestRule.setContent {
            TvButton(label = "Stop Sharing", onClick = {}, isDestructive = true)
        }
        composeTestRule.onNodeWithText("Stop Sharing").assertExists()
    }

    @Test
    fun leanbackScreen_showsStoppedStateInitially() {
        composeTestRule.setContent {
            TvLeanbackScreen(
                onStartServer = {},
                onStopService = {},
                onConnectToServer = { _, _ -> },
                discoveredServers = emptyList(),
                localDevices = emptyList(),
                isServiceRunning = false,
                serviceModeText = "",
            )
        }
        composeTestRule.onNodeWithText("Stopped").assertExists()
    }

    @Test
    fun leanbackScreen_showsRunningState() {
        composeTestRule.setContent {
            TvLeanbackScreen(
                onStartServer = {},
                onStopService = {},
                onConnectToServer = { _, _ -> },
                discoveredServers = emptyList(),
                localDevices = emptyList(),
                isServiceRunning = true,
                serviceModeText = "Server — sharing test",
            )
        }
        composeTestRule.onNodeWithText("Running — Server — sharing test").assertExists()
    }

    @Test
    fun leanbackScreen_showsStopButtonWhenRunning() {
        composeTestRule.setContent {
            TvLeanbackScreen(
                onStartServer = {},
                onStopService = {},
                onConnectToServer = { _, _ -> },
                discoveredServers = emptyList(),
                localDevices = emptyList(),
                isServiceRunning = true,
                serviceModeText = "test",
            )
        }
        composeTestRule.onNodeWithText("Stop").assertExists()
    }

    @Test
    fun leanbackScreen_listsLocalDevices() {
        val devices = listOf(
            LocalUsbDevice(name = "Test Drive", vid = 0x1234, pid = 0x5678),
        )
        composeTestRule.setContent {
            TvLeanbackScreen(
                onStartServer = {},
                onStopService = {},
                onConnectToServer = { _, _ -> },
                discoveredServers = emptyList(),
                localDevices = devices,
                isServiceRunning = false,
                serviceModeText = "",
            )
        }
        composeTestRule.onNodeWithText("Test Drive").assertExists()
        composeTestRule.onNodeWithText("1234:5678").assertExists()
    }

    @Test
    fun leanbackScreen_showsEmptyStateWhenNoDevices() {
        composeTestRule.setContent {
            TvLeanbackScreen(
                onStartServer = {},
                onStopService = {},
                onConnectToServer = { _, _ -> },
                discoveredServers = emptyList(),
                localDevices = emptyList(),
                isServiceRunning = false,
                serviceModeText = "",
            )
        }
        composeTestRule.onNodeWithText(
            "No USB devices found. Plug in a device and ensure USB Host mode is enabled.",
        ).assertExists()
    }

    @Test
    fun leanbackScreen_listsDiscoveredServers() {
        val servers = listOf(
            DiscoveredServer(
                host = "192.168.1.100",
                port = 3240,
                devices = listOf(
                    RemoteDevice("flash-drive", "1-1", 0x1234, 0x5678),
                ),
            ),
        )
        composeTestRule.setContent {
            TvLeanbackScreen(
                onStartServer = {},
                onStopService = {},
                onConnectToServer = { _, _ -> },
                discoveredServers = servers,
                localDevices = emptyList(),
                isServiceRunning = false,
                serviceModeText = "",
            )
        }
        composeTestRule.onNodeWithText("192.168.1.100").assertExists()
        composeTestRule.onNodeWithText("flash-drive (1-1)").assertExists()
    }

    @Test
    fun leanbackScreen_showsManualConnectionSection() {
        composeTestRule.setContent {
            TvLeanbackScreen(
                onStartServer = {},
                onStopService = {},
                onConnectToServer = { _, _ -> },
                discoveredServers = emptyList(),
                localDevices = emptyList(),
                isServiceRunning = false,
                serviceModeText = "",
            )
        }
        composeTestRule.onNodeWithText("Manual Connection").assertExists()
        composeTestRule.onNodeWithText("Connect").assertExists()
    }

    @Test
    fun tvStatusCard_showsRunningState() {
        composeTestRule.setContent {
            TvStatusCard(isRunning = true, modeText = "Client — connected")
        }
        composeTestRule.onNodeWithText("Running — Client — connected").assertExists()
    }

    @Test
    fun tvStatusCard_showsStoppedState() {
        composeTestRule.setContent {
            TvStatusCard(isRunning = false, modeText = "")
        }
        composeTestRule.onNodeWithText("Stopped").assertExists()
    }

    @Test
    fun tvStatusCard_showsStopButtonWhenRunning() {
        composeTestRule.setContent {
            TvStatusCard(
                isRunning = true,
                modeText = "test",
                onStopClick = {},
            )
        }
        composeTestRule.onNodeWithText("Stop").assertExists()
    }
}
