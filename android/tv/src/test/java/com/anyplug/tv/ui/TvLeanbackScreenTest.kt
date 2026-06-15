package com.anyplug.tv.ui

import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.anyplug.model.DiscoveredServer
import com.anyplug.model.LocalUsbDevice
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Tests for TV-optimized Leanback UI components.
 *
 * Verifies:
 * - Section headers render with correct text
 * - Device cards display name and VID:PID
 * - Empty state messages appear when no devices found
 * - Service status indicator shows correct state
 */
@RunWith(AndroidJUnit4::class)
class TvLeanbackScreenTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    @Test
    fun sectionHeader_displaysTitle() {
        composeTestRule.setContent {
            SectionHeader("Test Devices")
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
                onAction = {}
            )
        }
        composeTestRule.onNodeWithText("USB Device").assertExists()
        composeTestRule.onNodeWithText("1234:5678").assertExists()
        composeTestRule.onNodeWithText("Share").assertExists()
    }

    @Test
    fun tvEmptyMessage_showsProvidedMessage() {
        composeTestRule.setContent {
            TvEmptyMessage("No devices found. Connect a USB device.")
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
    fun leanbackScreen_showsStoppedStateInitially() {
        composeTestRule.setContent {
            TvLeanbackScreen(
                onStartServer = {},
                onConnectToServer = { _, _ -> },
                discoveredServers = emptyList(),
                localDevices = emptyList(),
                isServiceRunning = false,
                serviceModeText = ""
            )
        }
        composeTestRule.onNodeWithText("Stopped").assertExists()
    }

    @Test
    fun leanbackScreen_showsRunningState() {
        composeTestRule.setContent {
            TvLeanbackScreen(
                onStartServer = {},
                onConnectToServer = { _, _ -> },
                discoveredServers = emptyList(),
                localDevices = emptyList(),
                isServiceRunning = true,
                serviceModeText = "Server — sharing test"
            )
        }
        composeTestRule.onNodeWithText("Running — Server — sharing test").assertExists()
    }

    @Test
    fun leanbackScreen_listsLocalDevices() {
        val devices = listOf(
            LocalUsbDevice(name = "Test Drive", vid = 0x1234, pid = 0x5678)
        )
        composeTestRule.setContent {
            TvLeanbackScreen(
                onStartServer = {},
                onConnectToServer = { _, _ -> },
                discoveredServers = emptyList(),
                localDevices = devices,
                isServiceRunning = false,
                serviceModeText = ""
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
                onConnectToServer = { _, _ -> },
                discoveredServers = emptyList(),
                localDevices = emptyList(),
                isServiceRunning = false,
                serviceModeText = ""
            )
        }
        composeTestRule.onNodeWithText("No USB devices found. Plug in a device and ensure USB Host mode is enabled.")
            .assertExists()
    }

    @Test
    fun leanbackScreen_listsDiscoveredServers() {
        val servers = listOf(
            DiscoveredServer(
                host = "192.168.1.100",
                port = 3240,
                devices = listOf(
                    com.anyplug.model.RemoteDevice("flash-drive", "1-1", 0x1234, 0x5678)
                )
            )
        )
        composeTestRule.setContent {
            TvLeanbackScreen(
                onStartServer = {},
                onConnectToServer = { _, _ -> },
                discoveredServers = servers,
                localDevices = emptyList(),
                isServiceRunning = false,
                serviceModeText = ""
            )
        }
        composeTestRule.onNodeWithText("192.168.1.100").assertExists()
        composeTestRule.onNodeWithText("flash-drive (1-1)").assertExists()
    }
}
