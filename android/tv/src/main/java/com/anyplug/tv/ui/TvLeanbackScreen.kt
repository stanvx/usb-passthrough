package com.anyplug.tv.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.anyplug.model.DiscoveredServer
import com.anyplug.model.LocalUsbDevice
import com.anyplug.model.RemoteDevice

/**
 * TV-optimised main screen for AnyPlug TV.
 *
 * Designed for 10 ft viewing with:
 * - Dark M3 Expressive background
 * - Enlarged typography (24 sp–36 sp)
 * - Animated focus borders on D-pad navigation
 * - Minimum 48 dp touch targets, key items 64 dp+
 * - Focus group containers for natural D-pad flow
 */
@Composable
fun TvLeanbackScreen(
    onStartServer: (deviceName: String) -> Unit,
    onStopService: () -> Unit,
    onConnectToServer: (host: String, busId: String) -> Unit,
    discoveredServers: List<DiscoveredServer>,
    localDevices: List<LocalUsbDevice>,
    isServiceRunning: Boolean,
    serviceModeText: String,
    sharedDeviceName: String = "",
) {
    var showStorageWarning by remember { mutableStateOf<LocalUsbDevice?>(null) }
    val scrollState = rememberScrollState()
    val headerFocusRequester = remember { FocusRequester() }

    // Request initial focus on the header when the screen loads
    LaunchedEffect(Unit) {
        headerFocusRequester.requestFocus()
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .verticalScroll(scrollState)
            .padding(horizontal = 32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Spacer(modifier = Modifier.height(48.dp))

        // ── App title ──────────────────────────────────────────
        Text(
            text = "AnyPlug TV",
            style = MaterialTheme.typography.headlineLarge,
            fontWeight = FontWeight.Bold,
            color = MaterialTheme.colorScheme.onBackground,
            textAlign = TextAlign.Center,
            modifier = Modifier
                .focusRequester(headerFocusRequester)
                .focusable()
                .padding(vertical = 16.dp, horizontal = 8.dp),
        )

        Spacer(modifier = Modifier.height(20.dp))

        // ── Service status ─────────────────────────────────────
        TvStatusCard(
            isRunning = isServiceRunning,
            modeText = serviceModeText,
            onStopClick = if (isServiceRunning) onStopService else null,
        )

        Spacer(modifier = Modifier.height(48.dp))

        // ── Server section ─────────────────────────────────────
        TvSectionHeader("Server — Export USB Device")

        Spacer(modifier = Modifier.height(16.dp))

        if (localDevices.isEmpty()) {
            TvEmptyState(
                message = "No USB devices found. Plug in a device and " +
                    "ensure USB Host mode is enabled.",
            )
        } else {
            // Focus group for the device list
            TvFocusGroup {
                localDevices.forEach { device ->
                    val isThisDeviceShared = sharedDeviceName == device.name

                    TvDeviceCard(
                        title = device.name,
                        subtitle = "${device.vid.toString(16).padStart(4, '0')}:" +
                            device.pid.toString(16).padStart(4, '0'),
                        actionLabel = if (isThisDeviceShared) "Stop Sharing" else "Share",
                        isDestructive = isThisDeviceShared,
                        onAction = {
                            if (!isThisDeviceShared && device.isMassStorage) {
                                showStorageWarning = device
                            } else if (!isThisDeviceShared) {
                                onStartServer(device.name)
                            }
                        },
                    )
                }
            }
        }

        Spacer(modifier = Modifier.height(48.dp))

        // ── Client section ─────────────────────────────────────
        TvSectionHeader("Client — Connect to Remote Server")

        Spacer(modifier = Modifier.height(16.dp))

        if (discoveredServers.isEmpty()) {
            TvEmptyState(
                message = "No USB/IP servers found. Ensure mDNS is enabled " +
                    "on the network.",
            )
        } else {
            TvFocusGroup {
                discoveredServers.forEach { server ->
                    TvCard {
                        Column(modifier = Modifier.padding(20.dp)) {
                            Text(
                                text = server.host,
                                style = MaterialTheme.typography.titleMedium,
                                fontWeight = FontWeight.SemiBold,
                                modifier = Modifier.padding(bottom = 8.dp),
                            )

                            server.devices.forEach { device ->
                                Row(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .padding(vertical = 8.dp),
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Text(
                                        text = "${device.name} (${device.busId})",
                                        style = MaterialTheme.typography.bodyLarge,
                                        modifier = Modifier
                                            .weight(1f)
                                            .padding(end = 16.dp),
                                    )
                                    TvButton(
                                        label = "Connect",
                                        onClick = {
                                            onConnectToServer(server.host, device.busId)
                                        },
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }

        Spacer(modifier = Modifier.height(40.dp))

        // ── Manual connection ──────────────────────────────────
        TvSectionHeader("Manual Connection")

        Spacer(modifier = Modifier.height(16.dp))

        TvConnectionInput(
            onConnect = { host, busId ->
                onConnectToServer(host, busId)
            },
        )

        Spacer(modifier = Modifier.height(64.dp))
    }

    // ── Mass-storage warning dialog ────────────────────────────
    val warnedDevice = showStorageWarning
    if (warnedDevice != null) {
        AlertDialog(
            onDismissRequest = { showStorageWarning = null },
            title = { Text("Share Storage Device?") },
            text = {
                Text(
                    "This is a storage device. Sharing it will unmount it from " +
                    "your phone, which may cause an \"unsafely removed\" warning. " +
                    "\n\nTo avoid data loss, eject the storage in Android Settings " +
                    "before sharing.\n\nContinue anyway?"
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    showStorageWarning = null
                    onStartServer(warnedDevice.name)
                }) {
                    Text("Share Anyway")
                }
            },
            dismissButton = {
                TextButton(onClick = { showStorageWarning = null }) {
                    Text("Cancel")
                }
            },
        )
    }
}
