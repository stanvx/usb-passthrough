package com.anyplug.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable

/**
 * Custom M3 Expressive theme for AnyPlug (phone / tablet).
 *
 * Wraps [MaterialTheme] with the AnyPlug colour palette, typography,
 * and shapes. On Android 12+ the colour scheme is derived from the
 * user's wallpaper via dynamic colour (Material You); older devices
 * fall back to a hand-crafted M3 Expressive palette.
 *
 * Dark theme follows the system setting by default.
 *
 * Usage:
 * ```kotlin
 * AnyPlugTheme {
 *     // composable content
 * }
 * ```
 */
@Composable
fun AnyPlugTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    dynamicColor: Boolean = true,
    content: @Composable () -> Unit,
) {
    val colorScheme = anyPlugColorScheme(
        darkTheme = darkTheme,
        dynamicColor = dynamicColor,
    )

    MaterialTheme(
        colorScheme = colorScheme,
        typography = AnyPlugTypography,
        shapes = AnyPlugShapes,
        content = content,
    )
}
