package com.anyplug.tv.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Shapes
import androidx.compose.ui.unit.dp

/**
 * TV-optimised M3 shape scale for AnyPlug TV.
 *
 * Slightly rounder than the phone scale — cards and dialogs
 * use 20 dp radius for a softer, more approachable look on
 * large screens.
 */
val TvShapes = Shapes(
    extraSmall = RoundedCornerShape(6.dp),
    small = RoundedCornerShape(10.dp),
    medium = RoundedCornerShape(14.dp),
    large = RoundedCornerShape(20.dp),
    extraLarge = RoundedCornerShape(32.dp),
)
