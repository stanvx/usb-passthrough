package com.anyplug.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

/**
 * M3 Expressive color palette for AnyPlug.
 *
 * Uses vibrant, saturated tones that convey "bridging devices" —
 * electric blues anchor the primary palette, teal energises secondary,
 * and warm amber adds expressive contrast for accents and tertiary roles.
 *
 * On Android 12+, dynamic colour (Material You) is used by default,
 * with a hand-crafted fallback for older versions.
 */

// ── Light palette (hand-crafted fallback) ──────────────────────────────

private val LightColors = lightColorScheme(
    primary = Color(0xFF1A6DFF),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFD9E2FF),
    onPrimaryContainer = Color(0xFF001A41),

    secondary = Color(0xFF00897B),
    onSecondary = Color(0xFFFFFFFF),
    secondaryContainer = Color(0xFFB2DFDB),
    onSecondaryContainer = Color(0xFF002019),

    tertiary = Color(0xFFFF8F00),
    onTertiary = Color(0xFFFFFFFF),
    tertiaryContainer = Color(0xFFFFE0B2),
    onTertiaryContainer = Color(0xFF2C1A00),

    error = Color(0xFFBA1A1A),
    onError = Color(0xFFFFFFFF),
    errorContainer = Color(0xFFFFDAD6),
    onErrorContainer = Color(0xFF410002),

    background = Color(0xFFF9F9FF),
    onBackground = Color(0xFF1A1C20),
    surface = Color(0xFFF9F9FF),
    onSurface = Color(0xFF1A1C20),
    surfaceVariant = Color(0xFFE0E2EC),
    onSurfaceVariant = Color(0xFF44474E),
    outline = Color(0xFF74777F),
    outlineVariant = Color(0xFFC4C6D0),

    inverseSurface = Color(0xFF2F3036),
    inverseOnSurface = Color(0xFFF1F0F6),
    inversePrimary = Color(0xFFAFC6FF),
    surfaceTint = Color(0xFF1A6DFF),
)

// ── Dark palette (hand-crafted fallback) ───────────────────────────────

private val DarkColors = darkColorScheme(
    primary = Color(0xFF9ECAFF),
    onPrimary = Color(0xFF002E69),
    primaryContainer = Color(0xFF004396),
    onPrimaryContainer = Color(0xFFD9E2FF),

    secondary = Color(0xFF80CBC4),
    onSecondary = Color(0xFF003731),
    secondaryContainer = Color(0xFF005048),
    onSecondaryContainer = Color(0xFFB2DFDB),

    tertiary = Color(0xFFFFD54F),
    onTertiary = Color(0xFF4A2E00),
    tertiaryContainer = Color(0xFF694400),
    onTertiaryContainer = Color(0xFFFFE0B2),

    error = Color(0xFFFFB4AB),
    onError = Color(0xFF690005),
    errorContainer = Color(0xFF93000A),
    onErrorContainer = Color(0xFFFFDAD6),

    background = Color(0xFF121318),
    onBackground = Color(0xFFE3E2E9),
    surface = Color(0xFF121318),
    onSurface = Color(0xFFE3E2E9),
    surfaceVariant = Color(0xFF44474E),
    onSurfaceVariant = Color(0xFFC4C6D0),
    outline = Color(0xFF8E9099),
    outlineVariant = Color(0xFF44474E),

    inverseSurface = Color(0xFFE3E2E9),
    inverseOnSurface = Color(0xFF2F3036),
    inversePrimary = Color(0xFF1A6DFF),
    surfaceTint = Color(0xFF9ECAFF),
)

// ── Semantic alias tokens ──────────────────────────────────────────────

/** Status colour emitted when the service is actively running. */
val ColorScheme.runningGreen: Color
    get() = if (this == lightColorScheme()) Color(0xFF2E7D32) else Color(0xFF81C784)

/** Status colour emitted when the service is stopped / idle. */
val ColorScheme.stoppedGray: Color
    get() = Color(0xFF9E9E9E)

/** Surface elevation tint used for service-status cards. */
val ColorScheme.statusSurface: Color
    @Composable
    get() = if (isSystemInDarkTheme()) surfaceVariant else primaryContainer.copy(alpha = 0.15f)

/** Accent highlight for the currently focused item (TV-friendly). */
val ColorScheme.focusHighlight: Color
    get() = primary.copy(alpha = 0.30f)

// ── Theme resolver ─────────────────────────────────────────────────────

/**
 * Resolves the appropriate [ColorScheme] for the current context.
 *
 * - Android 12+ (API 31+): dynamic colour from wallpaper (Material You).
 * - Older devices: hand-crafted M3 Expressive palette.
 *
 * Dark theme follows the system setting.
 */
@Composable
fun anyPlugColorScheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    dynamicColor: Boolean = true,
): ColorScheme {
    if (dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        val context = LocalContext.current
        return if (darkTheme) dynamicDarkColorScheme(context)
        else dynamicLightColorScheme(context)
    }
    return if (darkTheme) DarkColors else LightColors
}
