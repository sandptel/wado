package io.github.sandptel.wado.ui.theme

import android.content.Context
import android.provider.Settings
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween

/**
 * DESIGN.md's motion, the same two curves as the web client (crates/client/assets/base.css):
 * `spring` settles like a sheet with mass, for sheets and pages; `pop` overshoots a touch, for
 * small things a finger just flipped. Durations from the same tokens; exits run at ~80% of the
 * entrance. With the system's animations off, everything snaps.
 */
object Motion {
    val Spring = CubicBezierEasing(.32f, .72f, 0f, 1f)
    val Pop = CubicBezierEasing(.34f, 1.56f, .64f, 1f)
    val Accelerate = CubicBezierEasing(.3f, 0f, 1f, 1f)

    const val DUR = 220
    const val DUR_SHEET = 480
    const val DUR_SHEET_EXIT = 420
    const val STAGGER = 40

    var reduced = false
        private set

    fun read(context: Context) {
        reduced = Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    }

    fun <T> spring(ms: Int = DUR, delay: Int = 0): FiniteAnimationSpec<T> =
        if (reduced) snap() else tween(ms, delay, Spring)

    fun <T> pop(ms: Int = DUR, delay: Int = 0): FiniteAnimationSpec<T> =
        if (reduced) snap() else tween(ms, delay, Pop)

    fun <T> exit(ms: Int = 180): FiniteAnimationSpec<T> =
        if (reduced) snap() else tween(ms, 0, Accelerate)
}
