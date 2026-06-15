package com.anyplug.tv.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.anyplug.model.DiscoveredServer
import com.anyplug.model.LocalUsbDevice
import com.anyplug.model.RemoteDevice

/**
 * TV-optimized Leanback-style main screen for AnyPlug.
 *
 * Features:
 * - Larger touch/focus targets (minimum 48dp, key items 64dp+)
 * - D-pad navigation with focusable row-based layout
 * - BrowseFragment-like header + row organization
 * - High contrast text for TV readability
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TvLeanbackScreen(
    onStartServer: (deviceName: String) -> Unit,
    onConnectToServer: (host: String, busId: String) -> Unit,
    discoveredServers: List<DiscoveredServer>,
    localDevices: List<LocalUsbDevice>,
    isServiceRunning: Boolean,
    serviceModeText: String
) {
    val scrollState = rememberScrollState()
    val headerFocusRequester = remember { FocusRequester() }

    LaunchedEffect(Unit) {
        headerFocusRequester.requestFocus()
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .verticalScroll(scrollState)
            .padding(horizontal = 32.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        // ── Header (BrowseFragment-style title) ───────────────
        Spacer(modifier = Modifier.height(48.dp))

        Text(
            text = "AnyPlug TV",
            style = MaterialTheme.typography.headlineLarge.copy(
                fontSize = 36.sp,
                fontWeight = FontWeight.Bold
            ),
            color = MaterialTheme.colorScheme.onBackground,
            modifier = Modifier
                .focusRequester(headerFocusRequester)
                .focusable()
                .padding(vertical = 16.dp)
        )

        Spacer(modifier = Modifier.height(16.dp))

        // ── Service Status Card ───────────────────────────────
        TvCard(
            modifier = Modifier.fillMaxWidth(),
            backgroundColor = if (isServiceRunning)
                MaterialTheme.colorScheme.tertiaryContainer
            else
                MaterialTheme.colorScheme.surfaceVariant
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 24.dp, vertical = 20.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = if (isServiceRunning) "Running — $serviceModeText"
                    else "Stopped",
                    style = MaterialTheme.typography.titleLarge.copy(fontSize = 24.sp),
                    modifier = Modifier
                        .focusable()
                        .padding(8.dp)
                )
            }
        }

        Spacer(modifier = Modifier.height(48.dp))

        // ── Server Row ────────────────────────────────────────
        SectionHeader("Server — Export USB Device")

        Spacer(modifier = Modifier.height(16.dp))

        if (localDevices.isEmpty()) {
            TvEmptyMessage("No USB devices found. Plug in a device and ensure USB Host mode is enabled.")
        } else {
            localDevices.forEach { device ->
                TvDeviceCard(
                    title = device.name,
                    subtitle = "${device.vid.toString(16).padStart(4, '0')}:" +
                        "${device.pid.toString(16).padStart(4, '0')}",
                    actionLabel = "Share",
                    onAction = { onStartServer(device.name) }
                )
            }
        }

        Spacer(modifier = Modifier.height(48.dp))

        // ── Client Row ────────────────────────────────────────
        SectionHeader("Client — Connect to Remote Server")

        Spacer(modifier = Modifier.height(16.dp))

        if (discoveredServers.isEmpty()) {
            TvEmptyMessage("No USB/IP servers found. Ensure mDNS is enabled on the network.")
        } else {
            discoveredServers.forEach { server ->
                TvCard {
                    Column(modifier = Modifier.padding(20.dp)) {
                        Text(
                            text = server.host,
                            style = MaterialTheme.typography.titleMedium.copy(fontSize = 20.sp),
                            fontWeight = FontWeight.SemiBold,
                            modifier = Modifier.focusable().padding(8.dp)
                        )

                        server.devices.forEach { device ->
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(vertical = 8.dp),
                                horizontalArrangement = Arrangement.SpaceBetween,
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Text(
                                    text = "${device.name} (${device.busId})",
                                    style = MaterialTheme.typography.bodyLarge.copy(fontSize = 18.sp),
                                    modifier = Modifier.weight(1f).padding(end = 16.dp)
                                )
                                TvButton(
                                    label = "Connect",
                                    onClick = { onConnectToServer(server.host, device.busId) }
                                )
                            }
                        }
                    }
                }
            }
        }

        Spacer(modifier = Modifier.height(32.dp))

        // ── Manual Connection ─────────────────────────────────
        SectionHeader("Manual Connection")

        Spacer(modifier = Modifier.height(16.dp))

        TvManualConnectionInput(
            onConnect = { host, busId ->
                onConnectToServer(host, busId)
            }
        )

        Spacer(modifier = Modifier.height(64.dp))
    }
}

/**
 * Section header styled like Leanback BrowseFragment row headers.
 * Larger text for TV readability from 10ft distance.
 */
@Composable
fun SectionHeader(title: String) {
    Text(
        text = title,
        style = MaterialTheme.typography.titleLarge.copy(
            fontSize = 28.sp,
            fontWeight = FontWeight.Bold
        ),
        color = MaterialTheme.colorScheme.onBackground,
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp)
            .focusable()
            .padding(horizontal = 8.dp, vertical = 4.dp)
    )
}

/**
 * Empty state message with large, readable text.
 */
@Composable
fun TvEmptyMessage(message: String) {
    TvCard {
        Text(
            text = message,
            style = MaterialTheme.typography.bodyLarge.copy(fontSize = 18.sp),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            modifier = Modifier
                .fillMaxWidth()
                .padding(24.dp)
                .focusable()
        )
    }
}

/**
 * TV-optimized card with minimum 64dp row height for easy D-pad targeting.
 */
@Composable
fun TvCard(
    modifier: Modifier = Modifier,
    backgroundColor: Color = MaterialTheme.colorScheme.surfaceVariant,
    content: @Composable ColumnScope.() -> Unit
) {
    Card(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp),
        colors = CardDefaults.cardColors(containerColor = backgroundColor)
    ) {
        Column(
            modifier = Modifier
                .focusable()
                .then(Modifier.fillMaxWidth()),
            content = content
        )
    }
}

/**
 * TV-optimized device card with a large action button (minimum 48dp height).
 */
@Composable
fun TvDeviceCard(
    title: String,
    subtitle: String,
    actionLabel: String,
    onAction: () -> Unit
) {
    TvCard {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp, vertical = 20.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium.copy(
                        fontSize = 20.sp,
                        fontWeight = FontWeight.SemiBold
                    ),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(vertical = 4.dp)
                )
                Text(
                    text = subtitle,
                    style = MaterialTheme.typography.bodyMedium.copy(fontSize = 16.sp),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(vertical = 4.dp)
                )
            }
            Spacer(modifier = Modifier.width(24.dp))
            TvButton(label = actionLabel, onClick = onAction)
        }
    }
}

/**
 * TV-optimized button with minimum 48dp height for easy D-pad targeting.
 */
@Composable
fun TvButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    Button(
        onClick = onClick,
        modifier = modifier
            .heightIn(min = 48.dp)
            .widthIn(min = 140.dp)
            .focusable(),
        colors = ButtonDefaults.buttonColors(
            containerColor = MaterialTheme.colorScheme.primary
        )
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.labelLarge.copy(fontSize = 18.sp),
            maxLines = 1,
            modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp)
        )
    }
}

/**
 * Manual connection input with host:port and bus ID fields.
 * Uses larger text field targets for TV remote input.
 */
@Composable
fun TvManualConnectionInput(
    onConnect: (host: String, busId: String) -> Unit
) {
    var hostPort by remember { mutableStateOf("") }
    var busId by remember { mutableStateOf("") }

    TvCard {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(24.dp)
        ) {
            OutlinedTextField(
                value = hostPort,
                onValueChange = { hostPort = it },
                label = { Text("Server (host:port)", style = MaterialTheme.typography.bodyLarge) },
                placeholder = { Text("192.168.1.100:3240") },
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 56.dp)
                    .focusable(),
                singleLine = true,
                textStyle = MaterialTheme.typography.bodyLarge.copy(fontSize = 18.sp)
            )

            Spacer(modifier = Modifier.height(16.dp))

            OutlinedTextField(
                value = busId,
                onValueChange = { busId = it },
                label = { Text("Bus ID", style = MaterialTheme.typography.bodyLarge) },
                placeholder = { Text("1-1") },
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 56.dp)
                    .focusable(),
                singleLine = true,
                textStyle = MaterialTheme.typography.bodyLarge.copy(fontSize = 18.sp)
            )

            Spacer(modifier = Modifier.height(20.dp))

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End
            ) {
                TvButton(
                    label = "Connect",
                    onClick = {
                        if (hostPort.isNotBlank() && busId.isNotBlank()) {
                            onConnect(hostPort, busId)
                        }
                    },
                    modifier = Modifier.widthIn(min = 180.dp)
                )
            }
        }
    }
}
