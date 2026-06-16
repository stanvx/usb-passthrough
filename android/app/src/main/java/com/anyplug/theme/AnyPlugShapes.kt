package com.anyplug.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Shapes
import androidx.compose.ui.unit.dp

/**
 * M3 Expressive shape scale for AnyPlug.
 *
 * Slightly rounder than the default M3 shapes to convey
 * approachability — consistent with the "bridging devices"
 * product ethos. Extra-small is a subtle 4 dp, while
 * extra-large rounds fully to 28 dp for cards and dialogs.
 */
val AnyPlugShapes = Shapes(
    extraSmall = RoundedCornerShape(4.dp),
    small = RoundedCornerShape(8.dp),
    medium = RoundedCornerShape(12.dp),
    large = RoundedCornerShape(16.dp),
    extraLarge = RoundedCornerShape(28.dp),
)
