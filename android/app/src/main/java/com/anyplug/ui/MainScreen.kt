package com.anyplug.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.anyplug.model.DiscoveredServer
import com.anyplug.model.LocalUsbDevice
import com.anyplug.ui.components.DeviceCard
import com.anyplug.ui.components.EmptyState
import com.anyplug.ui.components.SectionHeader
import com.anyplug.ui.components.StatusCard

/**
 * Main screen for the AnyPlug phone / tablet app.
 *
 * Uses the M3 Expressive [AnyPlugTheme] tokens through [MaterialTheme].
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MainScreen(
    onStartServer: (deviceName: String) -> Unit,
    onStopService: () -> Unit,
    onConnectToServer: (host: String, busId: String) -> Unit,
    discoveredServers: List<DiscoveredServer>,
    localDevices: List<LocalUsbDevice>,
    isServiceRunning: Boolean,
    serviceModeText: String,
    sharedDeviceName: String = "",
) {
    var selectedTab by remember { mutableStateOf(0) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("AnyPlug") },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primaryContainer,
                    titleContentColor = MaterialTheme.colorScheme.onPrimaryContainer,
                ),
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
        ) {
            // ── Service status (always visible — stable layout) ──
            StatusCard(
                isRunning = isServiceRunning,
                modeText = serviceModeText,
                onStopClick = if (isServiceRunning) onStopService else null,
            )

            Spacer(modifier = Modifier.height(20.dp))

            // ── Tab row ─────────────────────────────────────────
            TabRow(selectedTabIndex = selectedTab) {
                Tab(
                    selected = selectedTab == 0,
                    onClick = { selectedTab = 0 },
                    text = { Text("Server", modifier = Modifier.padding(12.dp)) },
                )
                Tab(
                    selected = selectedTab == 1,
                    onClick = { selectedTab = 1 },
                    text = { Text("Client", modifier = Modifier.padding(12.dp)) },
                )
            }

            Spacer(modifier = Modifier.height(16.dp))

            when (selectedTab) {
                0 -> ServerPanel(
                    localDevices = localDevices,
                    onStartServer = onStartServer,
                    sharedDeviceName = sharedDeviceName,
                )
                1 -> ClientPanel(discoveredServers, onConnectToServer)
            }
        }
    }
}

// ── Server panel ───────────────────────────────────────────────────────

@Composable
private fun ServerPanel(
    localDevices: List<LocalUsbDevice>,
    onStartServer: (String) -> Unit,
    sharedDeviceName: String,
) {
    var showStorageWarning by remember { mutableStateOf<LocalUsbDevice?>(null) }

    SectionHeader("Local USB Devices")

    Spacer(modifier = Modifier.height(8.dp))

    if (localDevices.isEmpty()) {
        EmptyState(
            message = "No USB devices found. Plug in a device and ensure " +
                "USB Host mode is enabled.",
        )
    } else {
        localDevices.forEach { device ->
            val isThisDeviceShared = sharedDeviceName == device.name

            DeviceCard(
                title = device.name,
                subtitle = "${device.vid.toString(16).padStart(4, '0')}:" +
                    device.pid.toString(16).padStart(4, '0'),
                actionLabel = "Share",
                isShared = isThisDeviceShared,
                onAction = {
                    if (!isThisDeviceShared && device.isMassStorage) {
                        showStorageWarning = device
                    } else if (!isThisDeviceShared) {
                        onStartServer(device.name)
                    }
                },
                modifier = Modifier.padding(vertical = 4.dp),
            )
        }
    }

    // ── Mass-storage warning dialog ─────────────────────────
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

// ── Client panel ───────────────────────────────────────────────────────

@Composable
private fun ClientPanel(
    discoveredServers: List<DiscoveredServer>,
    onConnect: (host: String, busId: String) -> Unit,
) {
    var manualHost by remember { mutableStateOf("") }
    var manualBusId by remember { mutableStateOf("") }

    SectionHeader("Discovered Servers")

    Spacer(modifier = Modifier.height(8.dp))

    if (discoveredServers.isEmpty()) {
        EmptyState(
            message = "No USB/IP servers found. Ensure mDNS is enabled on the network.",
        )
    } else {
        discoveredServers.forEach { server ->
            val firstDevice = server.devices.firstOrNull()
            DeviceCard(
                title = server.host,
                subtitle = server.devices.joinToString(", ") { device ->
                    "${device.name} (${device.busId})"
                },
                actionLabel = "Connect",
                onAction = {
                    if (firstDevice != null) {
                        onConnect(server.host, firstDevice.busId)
                    } else {
                        onConnect(server.host, "1-1")
                    }
                },
                modifier = Modifier.padding(vertical = 4.dp),
            )
        }
    }

    Spacer(modifier = Modifier.height(20.dp))

    // ── Manual connection ──────────────────────────────────────
    SectionHeader("Manual Connection")

    Spacer(modifier = Modifier.height(8.dp))

    OutlinedTextField(
        value = manualHost,
        onValueChange = { manualHost = it },
        label = { Text("Server address (host:port)") },
        placeholder = { Text("e.g. 192.168.1.100:3240") },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )

    Spacer(modifier = Modifier.height(8.dp))

    OutlinedTextField(
        value = manualBusId,
        onValueChange = { manualBusId = it },
        label = { Text("Bus ID") },
        placeholder = { Text("e.g. 1-1") },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )

    Spacer(modifier = Modifier.height(12.dp))

    val canSubmit = manualHost.isNotBlank() && manualBusId.isNotBlank()
    Button(
        onClick = { onConnect(manualHost, manualBusId) },
        enabled = canSubmit,
        colors = ButtonDefaults.buttonColors(
            containerColor = MaterialTheme.colorScheme.primary,
            contentColor = MaterialTheme.colorScheme.onPrimary,
        ),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Text(
            text = "Connect",
            fontWeight = FontWeight.SemiBold,
        )
    }
}
