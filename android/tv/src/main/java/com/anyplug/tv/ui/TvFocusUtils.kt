package com.anyplug.tv.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Duration of the focus-indicator animation (ms). */
private const val FOCUS_ANIM_MS = 180

/** Width of the focus-highlight border. */
private val FOCUS_BORDER_WIDTH = 3.dp

/**
 * Wraps [content] in a focusable [Box] that shows an animated focus border
 * when it gains D-pad / remote focus.
 *
 * The border animates its colour and a subtle background tint fades in
 * behind the content, providing clear visual feedback for TV navigation.
 *
 * @param shape Corner radius of the border.
 * @param borderWidth Width of the focus-highlight border in dp.
 * @param focusColor Colour of the border when focused.
 * @param unfocusedAlpha Alpha of the border when not focused (0 = invisible).
 * @param autoFocus Whether this box is itself focusable (default true).
 *                  Set to false when wrapping a child that is already
 *                  focusable (e.g. a Button).
 */
@Composable
fun TvFocusBox(
    modifier: Modifier = Modifier,
    shape: RoundedCornerShape = RoundedCornerShape(8.dp),
    borderWidth: Dp = FOCUS_BORDER_WIDTH,
    focusColor: Color = MaterialTheme.colorScheme.primary,
    unfocusedAlpha: Float = 0f,
    autoFocus: Boolean = true,
    content: @Composable () -> Unit,
) {
    val unfocusedColor = focusColor.copy(alpha = unfocusedAlpha)

    var focused by remember { mutableStateOf(false) }
    val borderColor by animateColorAsState(
        targetValue = if (focused) focusColor else unfocusedColor,
        animationSpec = tween(durationMillis = FOCUS_ANIM_MS),
        label = "focusBorder",
    )
    val bgTint by animateColorAsState(
        targetValue = if (focused) focusColor.copy(alpha = 0.10f) else Color.Transparent,
        animationSpec = tween(durationMillis = FOCUS_ANIM_MS),
        label = "focusBg",
    )

    Box(
        modifier = modifier
            .onFocusChanged { focused = it.isFocused }
            .then(if (autoFocus) Modifier.focusable() else Modifier)
            .border(borderWidth, borderColor, shape)
            .background(bgTint, shape),
    ) {
        content()
    }
}

/**
 * A container that groups focusable children, ensuring D-pad navigation
 * flows naturally within the group before moving to the next group.
 *
 * Use to group related items (e.g. a list of device cards) so the
 * D-pad navigates through all items in the group before advancing
 * to the next section.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun TvFocusGroup(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Box(modifier = modifier.focusGroup()) {
        content()
    }
}
