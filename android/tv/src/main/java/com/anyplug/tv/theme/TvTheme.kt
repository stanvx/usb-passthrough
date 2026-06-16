package com.anyplug.tv.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable

/**
 * Custom M3 dark theme for AnyPlug TV.
 *
 * Always uses a dark background — TV UIs are viewed in
 * low-light living rooms where a light theme causes eye
 * strain. All typography sizes are bumped for 10 ft readability.
 *
 * Usage:
 * ```kotlin
 * TvTheme {
 *     // composable content
 * }
 * ```
 */
@Composable
fun TvTheme(
    content: @Composable () -> Unit,
) {
    MaterialTheme(
        colorScheme = tvColorScheme(),
        typography = TvTypography,
        shapes = TvShapes,
        content = content,
    )
}
