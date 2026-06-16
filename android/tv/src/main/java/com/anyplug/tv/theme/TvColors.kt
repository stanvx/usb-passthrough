package com.anyplug.tv.theme

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color

/**
 * TV-optimised M3 Expressive colour palette for AnyPlug TV.
 *
 * TV screens are viewed from 10 ft (3 m) in a living-room
 * environment, so contrast ratios are pushed higher than the
 * phone palette. The background is always dark to avoid
 * eye strain in low-light rooms.
 *
 * The primary accent is an electric cyan-blue that reads well
 * on both OLED and LCD TV panels.
 */

// ── Dark (always-on — TV never uses light theme) ───────────────────────

private val TvDarkColors = darkColorScheme(
    primary = Color(0xFF80D0FF),
    onPrimary = Color(0xFF003549),
    primaryContainer = Color(0xFF004D6A),
    onPrimaryContainer = Color(0xFFC7E7FF),

    secondary = Color(0xFF6CD9CC),
    onSecondary = Color(0xFF003731),
    secondaryContainer = Color(0xFF005048),
    onSecondaryContainer = Color(0xFFA7F5E8),

    tertiary = Color(0xFFFFBC6B),
    onTertiary = Color(0xFF4A2900),
    tertiaryContainer = Color(0xFF6A3D00),
    onTertiaryContainer = Color(0xFFFFDCC2),

    error = Color(0xFFFFB4AB),
    onError = Color(0xFF690005),
    errorContainer = Color(0xFF93000A),
    onErrorContainer = Color(0xFFFFDAD6),

    background = Color(0xFF0F1118),
    onBackground = Color(0xFFE3E2E9),
    surface = Color(0xFF0F1118),
    onSurface = Color(0xFFE3E2E9),
    surfaceVariant = Color(0xFF3F4249),
    onSurfaceVariant = Color(0xFFBFC2CB),
    outline = Color(0xFF898C95),
    outlineVariant = Color(0xFF3F4249),

    inverseSurface = Color(0xFFE3E2E9),
    inverseOnSurface = Color(0xFF2F3036),
    inversePrimary = Color(0xFF006A92),
    surfaceTint = Color(0xFF80D0FF),
)

// ── Semantics ──────────────────────────────────────────────────────────

/**
 * Extra-bright highlight used for the focused item border.
 */
val focusHighlightColor: Color
    @Composable
    get() = TvDarkColors.primary.copy(alpha = 0.50f)

/**
 * Resolve the TV colour scheme.
 *
 * Always returns the dark variant — TV UIs are dark by nature.
 */
@Composable
fun tvColorScheme(): ColorScheme = TvDarkColors
