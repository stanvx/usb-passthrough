package com.anyplug.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.anyplug.theme.runningGreen
import com.anyplug.theme.stoppedGray

/**
 * Reusable design-system components for AnyPlug.
 *
 * Every component draws its tokens from [MaterialTheme] so they
 * automatically adapt to the M3 Expressive theme.
 */

// ── Status indicator ───────────────────────────────────────────────────

/**
 * A compact card showing the current service status.
 *
 * - Running    → green dot + descriptive text
 * - Stopped    → gray dot + "Stopped"
 */
@Composable
fun StatusCard(
    isRunning: Boolean,
    modeText: String,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = if (isRunning)
                MaterialTheme.colorScheme.primaryContainer
            else
                MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            // Status dot
            Text(
                text = "•",      // bullet
                color = if (isRunning) MaterialTheme.colorScheme.runningGreen
                else MaterialTheme.colorScheme.stoppedGray,
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.Bold,
            )
            Text(
                text = if (isRunning) "Running — $modeText" else "Stopped",
                style = MaterialTheme.typography.titleMedium,
            )
        }
    }
}

// ── Device card ────────────────────────────────────────────────────────

/**
 * A row-style card showing a USB device and its share / connect action.
 */
@Composable
fun DeviceCard(
    title: String,
    subtitle: String,
    actionLabel: String,
    onAction: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface,
        ),
        elevation = CardDefaults.cardElevation(defaultElevation = 1.dp),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleSmall,
                )
                Spacer(modifier = Modifier.height(2.dp))
                Text(
                    text = subtitle,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Button(
                onClick = onAction,
                contentPadding = ButtonDefaults.TextButtonContentPadding,
            ) {
                Text(text = actionLabel)
            }
        }
    }
}

// ── Section header ─────────────────────────────────────────────────────

/**
 * Standard section title used to separate UI areas.
 */
@Composable
fun SectionHeader(
    title: String,
    modifier: Modifier = Modifier,
) {
    Text(
        text = title,
        style = MaterialTheme.typography.titleMedium,
        color = MaterialTheme.colorScheme.primary,
        fontWeight = FontWeight.SemiBold,
        modifier = modifier.padding(top = 8.dp, bottom = 4.dp),
    )
}

// ── Empty state ────────────────────────────────────────────────────────

/**
 * Placeholder shown when a list has no items.
 */
@Composable
fun EmptyState(
    message: String,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f),
        ),
    ) {
        Text(
            text = message,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier
                .fillMaxWidth()
                .padding(24.dp),
        )
    }
}

// ── Status dot (small inline) ──────────────────────────────────────────

/**
 * A small coloured circle used inline to indicate running / stopped state.
 */
@Composable
fun StatusDot(
    isRunning: Boolean,
    modifier: Modifier = Modifier,
) {
    val color = if (isRunning) MaterialTheme.colorScheme.runningGreen
    else MaterialTheme.colorScheme.stoppedGray
    Text(
        text = "•",
        color = color,
        fontWeight = FontWeight.Bold,
        style = MaterialTheme.typography.titleLarge,
        modifier = modifier,
    )
}
