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
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
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
    onConnectToServer: (host: String, busId: String) -> Unit,
    discoveredServers: List<DiscoveredServer>,
    localDevices: List<LocalUsbDevice>,
    isServiceRunning: Boolean,
    serviceModeText: String,
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
            // ── Service status ──────────────────────────────────
            StatusCard(
                isRunning = isServiceRunning,
                modeText = serviceModeText,
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
                0 -> ServerPanel(localDevices, onStartServer)
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
) {
    SectionHeader("Local USB Devices")

    Spacer(modifier = Modifier.height(8.dp))

    if (localDevices.isEmpty()) {
        EmptyState(
            message = "No USB devices found. Plug in a device and ensure " +
                "USB Host mode is enabled.",
        )
    } else {
        localDevices.forEach { device ->
            DeviceCard(
                title = device.name,
                subtitle = "${device.vid.toString(16).padStart(4, '0')}:" +
                    device.pid.toString(16).padStart(4, '0'),
                actionLabel = "Share",
                onAction = { onStartServer(device.name) },
                modifier = Modifier.padding(vertical = 4.dp),
            )
        }
    }
}

// ── Client panel ───────────────────────────────────────────────────────

@Composable
private fun ClientPanel(
    discoveredServers: List<DiscoveredServer>,
    onConnect: (host: String, busId: String) -> Unit,
) {
    var manualHost by remember { mutableStateOf("") }

    SectionHeader("Discovered Servers")

    Spacer(modifier = Modifier.height(8.dp))

    if (discoveredServers.isEmpty()) {
        EmptyState(
            message = "No USB/IP servers found. Ensure mDNS is enabled on the network.",
        )
    } else {
        discoveredServers.forEach { server ->
            DeviceCard(
                title = server.host,
                subtitle = server.devices.joinToString(", ") { device ->
                    "${device.name} (${device.busId})"
                },
                actionLabel = "Connect",
                onAction = {},
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
}
