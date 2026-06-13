package com.anyplug.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/**
 * Main screen for the AnyPlug app.
 *
 * Shows a toggle between Server and Client mode, lists discovered
 * devices (mDNS), and provides connection controls.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MainScreen(
    onStartServer: (deviceName: String) -> Unit,
    onConnectToServer: (host: String, busId: String) -> Unit,
    discoveredServers: List<DiscoveredServer>,
    localDevices: List<LocalUsbDevice>,
    isServiceRunning: Boolean,
    serviceModeText: String
) {
    var selectedTab by remember { mutableStateOf(0) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("AnyPlug") },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primaryContainer
                )
            )
        }
    ) { padding ->
        Column(modifier = Modifier.padding(padding).padding(16.dp)) {
            // Service status
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(
                    containerColor = if (isServiceRunning)
                        MaterialTheme.colorScheme.tertiaryContainer
                    else
                        MaterialTheme.colorScheme.surfaceVariant
                )
            ) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Text(
                        text = if (isServiceRunning) "● Running — $serviceModeText"
                        else "○ Stopped",
                        style = MaterialTheme.typography.titleMedium
                    )
                }
            }

            Spacer(modifier = Modifier.height(16.dp))

            // Tab row
            TabRow(selectedTabIndex = selectedTab) {
                Tab(selected = selectedTab == 0, onClick = { selectedTab = 0 }) {
                    Text("Server", modifier = Modifier.padding(12.dp))
                }
                Tab(selected = selectedTab == 1, onClick = { selectedTab = 1 }) {
                    Text("Client", modifier = Modifier.padding(12.dp))
                }
            }

            Spacer(modifier = Modifier.height(16.dp))

            when (selectedTab) {
                0 -> ServerPanel(
                    localDevices = localDevices,
                    onStartServer = onStartServer
                )
                1 -> ClientPanel(
                    discoveredServers = discoveredServers,
                    onConnect = onConnectToServer
                )
            }
        }
    }
}

@Composable
fun ServerPanel(
    localDevices: List<LocalUsbDevice>,
    onStartServer: (String) -> Unit
) {
    Text(
        "Local USB Devices",
        style = MaterialTheme.typography.titleMedium
    )

    if (localDevices.isEmpty()) {
        Text(
            "No USB devices found. Plug in a device and ensure USB Host mode is enabled.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    } else {
        localDevices.forEach { device ->
            Card(
                modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)
            ) {
                Row(
                    modifier = Modifier.padding(16.dp),
                    horizontalArrangement = Arrangement.SpaceBetween
                ) {
                    Column {
                        Text(
                            device.name,
                            style = MaterialTheme.typography.titleSmall
                        )
                        Text(
                            "${device.vid.toString(16).padStart(4, '0')}:" +
                            "${device.pid.toString(16).padStart(4, '0')}",
                            style = MaterialTheme.typography.bodySmall
                        )
                    }
                    Button(onClick = { onStartServer(device.name) }) {
                        Text("Share")
                    }
                }
            }
        }
    }
}

@Composable
fun ClientPanel(
    discoveredServers: List<DiscoveredServer>,
    onConnect: (host: String, busId: String) -> Unit
) {
    var manualHost by remember { mutableStateOf("") }

    Text(
        "Discovered Servers",
        style = MaterialTheme.typography.titleMedium
    )

    if (discoveredServers.isEmpty()) {
        Text(
            "No USB/IP servers found. Ensure mDNS is enabled on the network.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    } else {
        discoveredServers.forEach { server ->
            Card(
                modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)
            ) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Text(server.host, style = MaterialTheme.typography.titleSmall)

                    server.devices.forEach { device ->
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Text(
                                "${device.name} (${device.busId})",
                                style = MaterialTheme.typography.bodySmall
                            )
                            Button(
                                onClick = { onConnect(server.host, device.busId) },
                                modifier = Modifier.height(32.dp)
                            ) {
                                Text("Connect", style = MaterialTheme.typography.labelSmall)
                            }
                        }
                    }
                }
            }
        }
    }

    Spacer(modifier = Modifier.height(16.dp))

    // Manual connection
    OutlinedTextField(
        value = manualHost,
        onValueChange = { manualHost = it },
        label = { Text("Manual server (host:port)") },
        modifier = Modifier.fillMaxWidth()
    )
}

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
