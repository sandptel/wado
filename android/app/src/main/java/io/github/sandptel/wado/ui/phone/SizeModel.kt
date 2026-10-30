package io.github.sandptel.wado.ui.phone

import androidx.compose.foundation.layout.sizeIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * DESIGN.md's Size, with Android's own pair: **Text size** (85–150 % in 5 % steps, or follow the
 * phone) and **Interface size** (Compact 0.9 · Cozy 1 · Comfy 1.1 · Large 1.2). [SizeScope] wraps
 * pages and sheets only — never the stage, the pad or a gamepad.
 */
data class Size(val text: Float = 1f, val follow: Boolean = true, val ui: Float = 1f)

val LocalSize = staticCompositionLocalOf { Size() }

object SizeModel {
    val TEXT_STEPS: List<Float> = (85..150 step 5).map { it / 100f }
    val UI_STEPS = listOf("Compact" to 0.9f, "Cozy" to 1f, "Comfy" to 1.1f, "Large" to 1.2f)

    /** The page's density: everything × interface, text × (the phone's scale, or the chosen one). */
    fun density(base: Density, size: Size, systemFontScale: Float): Density =
        Density(base.density * size.ui, if (size.follow) systemFontScale else size.text)

    /** A tap target's size in this scope, so that it is 44 dp on the glass whatever the interface size. */
    fun tapTarget(size: Size): Dp = (44f / size.ui).dp

    /** The nearest text step. */
    fun snapText(v: Float): Float = TEXT_STEPS.minBy { kotlin.math.abs(it - v) }
}

@Composable
fun SizeScope(size: Size, content: @Composable () -> Unit) {
    val base = LocalDensity.current
    val system = LocalConfiguration.current.fontScale
    CompositionLocalProvider(
        LocalDensity provides SizeModel.density(base, size, system),
        LocalSize provides size,
    ) { content() }
}

/** At least 44 dp on the glass, inside a [SizeScope]. */
fun Modifier.tap(): Modifier = composed {
    val t = SizeModel.tapTarget(LocalSize.current)
    sizeIn(minWidth = t, minHeight = t)
}
