package com.anyplug.tv.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

/**
 * TV-optimised design system components for AnyPlug TV.
 *
 * Every component:
 * - Uses enlarged typography for 10 ft readability
 * - Has minimum D-pad touch targets (48 dp+)
 * - Supports [TvFocusBox] for clear focus indication
 * - Uses the dark [TvTheme] colour scheme
 */

// ── TV Button ─────────────────────────────────────────────────────────

/**
 * Large, focusable button for TV remote navigation.
 *
 * Minimum touch target: 48 dp height, 140 dp width.
 */
@Composable
fun TvButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    isDestructive: Boolean = false,
) {
    Button(
        onClick = onClick,
        modifier = modifier
            .heightIn(min = 48.dp)
            .widthIn(min = 140.dp),
        colors = ButtonDefaults.buttonColors(
            containerColor = if (isDestructive)
                MaterialTheme.colorScheme.error
            else
                MaterialTheme.colorScheme.primary,
            contentColor = if (isDestructive)
                MaterialTheme.colorScheme.onError
            else
                MaterialTheme.colorScheme.onPrimary,
        ),
        shape = RoundedCornerShape(8.dp),
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.labelLarge,
            maxLines = 1,
        )
    }
}

// ── TV Card ────────────────────────────────────────────────────────────

/**
 * Card wrapper with focus-highlight support.
 *
 * The card's content is wrapped in a [TvFocusBox] so it glows
 * when selected via D-pad.
 *
 * @param backgroundColor Fill colour when not focused.
 */
@Composable
fun TvCard(
    modifier: Modifier = Modifier,
    backgroundColor: androidx.compose.ui.graphics.Color = MaterialTheme.colorScheme.surfaceVariant,
    content: @Composable () -> Unit,
) {
    TvFocusBox(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp),
        shape = RoundedCornerShape(12.dp),
    ) {
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = backgroundColor),
            shape = RoundedCornerShape(12.dp),
        ) {
            content()
        }
    }
}

// ── Device Card ────────────────────────────────────────────────────────

/**
 * Row-style device card with title, subtitle, and action button.
 * Large targets enable easy D-pad selection.
 *
 * @param isDestructive When true, the action button uses error/destructive
 *   styling (used for "Stop Sharing" state).
 */
@Composable
fun TvDeviceCard(
    title: String,
    subtitle: String,
    actionLabel: String,
    onAction: () -> Unit,
    isDestructive: Boolean = false,
) {
    TvCard {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp, vertical = 20.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(modifier = Modifier.height(4.dp))
                Text(
                    text = subtitle,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Spacer(modifier = Modifier.width(24.dp))
            TvButton(
                label = actionLabel,
                onClick = onAction,
                isDestructive = isDestructive,
            )
        }
    }
}

// ── Section Header ─────────────────────────────────────────────────────

/**
 * Full-width section title, focusable via D-pad.
 */
@Composable
fun TvSectionHeader(
    title: String,
    modifier: Modifier = Modifier,
) {
    Text(
        text = title,
        style = MaterialTheme.typography.headlineSmall,
        fontWeight = FontWeight.Bold,
        color = MaterialTheme.colorScheme.onBackground,
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp, horizontal = 8.dp),
    )
}

// ── Empty State ────────────────────────────────────────────────────────

/**
 * Empty-state placeholder with large readable text.
 */
@Composable
fun TvEmptyState(
    message: String,
    modifier: Modifier = Modifier,
) {
    TvCard(modifier = modifier) {
        Text(
            text = message,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            modifier = Modifier
                .fillMaxWidth()
                .padding(24.dp),
        )
    }
}

// ── Status Card ────────────────────────────────────────────────────────

/**
 * Compact card showing the current USB/IP service status.
 *
 * Uses a green/gray dot indicator with descriptive text.
 * Shows a "Stop" button when the service is running.
 *
 * The card is always visible to keep the layout stable.
 */
@Composable
fun TvStatusCard(
    isRunning: Boolean,
    modeText: String,
    modifier: Modifier = Modifier,
    onStopClick: (() -> Unit)? = null,
) {
    val dotColor = if (isRunning)
        MaterialTheme.colorScheme.primary
    else
        MaterialTheme.colorScheme.onSurfaceVariant

    TvCard(
        modifier = modifier,
        backgroundColor = if (isRunning)
            MaterialTheme.colorScheme.primaryContainer
        else
            MaterialTheme.colorScheme.surfaceVariant,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp, vertical = 20.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            // Status dot (large bullet)
            Text(
                text = "●",
                color = dotColor,
                style = MaterialTheme.typography.titleLarge,
            )
            // Status text — takes remaining space
            Text(
                text = if (isRunning) "Running — $modeText" else "Stopped",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.weight(1f),
            )
            // Stop button — only shown when running
            if (onStopClick != null) {
                TextButton(onClick = onStopClick) {
                    Text(
                        text = "Stop",
                        color = MaterialTheme.colorScheme.error,
                        fontWeight = FontWeight.SemiBold,
                        style = MaterialTheme.typography.titleSmall,
                    )
                }
            }
        }
    }
}

// ── Manual Connection Input ────────────────────────────────────────────

/**
 * Two-field form for manual server connection on TV.
 * Text fields are enlarged for TV remote text entry.
 */
@Composable
fun TvConnectionInput(
    onConnect: (host: String, busId: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var hostPort by remember { mutableStateOf("") }
    var busId by remember { mutableStateOf("") }

    TvCard(modifier = modifier) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(24.dp),
        ) {
            OutlinedTextField(
                value = hostPort,
                onValueChange = { hostPort = it },
                label = { Text("Server (host:port)", style = MaterialTheme.typography.bodyLarge) },
                placeholder = { Text("192.168.1.100:3240") },
                singleLine = true,
                textStyle = MaterialTheme.typography.bodyLarge,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 56.dp),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = MaterialTheme.colorScheme.primary,
                ),
            )

            Spacer(modifier = Modifier.height(16.dp))

            OutlinedTextField(
                value = busId,
                onValueChange = { busId = it },
                label = { Text("Bus ID", style = MaterialTheme.typography.bodyLarge) },
                placeholder = { Text("1-1") },
                singleLine = true,
                textStyle = MaterialTheme.typography.bodyLarge,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 56.dp),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = MaterialTheme.colorScheme.primary,
                ),
            )

            Spacer(modifier = Modifier.height(20.dp))

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
            ) {
                TvButton(
                    label = "Connect",
                    onClick = {
                        if (hostPort.isNotBlank() && busId.isNotBlank()) {
                            onConnect(hostPort, busId)
                        }
                    },
                    modifier = Modifier.widthIn(min = 180.dp),
                )
            }
        }
    }
}
