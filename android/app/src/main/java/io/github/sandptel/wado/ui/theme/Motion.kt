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
 * entrance. The person's motion level (full / reduced / off, Appearance) is AND-ed with the
 * system's animator scale: either one turning motion down wins.
 */
object Motion {
    val Spring = CubicBezierEasing(.32f, .72f, 0f, 1f)
    val Pop = CubicBezierEasing(.34f, 1.56f, .64f, 1f)
    val Accelerate = CubicBezierEasing(.3f, 0f, 1f, 1f)

    const val DUR = 220
    const val DUR_SHEET = 480
    const val DUR_SHEET_EXIT = 420
    /** A page popping back (M17's exit). */
    const val DUR_POP = 340
    const val DUR_HERO = 520
    const val DUR_PRESS = 120
    const val DUR_STAGE = 620
    const val DUR_STAGE_OUT = 520
    const val STAGGER = 40

    /** The reduced set, in the order DUR, DUR_SHEET, DUR_SHEET_EXIT, DUR_PRESS, STAGGER, DUR_HERO, DUR_STAGE. */
    val REDUCED = intArrayOf(120, 200, 160, 120, 0, 160, 160)

    /** The person's level: `full`, `reduced` or `off`. */
    var level = "full"
        private set
    private var systemOff = false

    /** Motion is off, or reduced: what most call sites need to know (they snap or fade). */
    val reduced get() = systemOff || level != "full"

    /** Nothing moves at all. */
    val off get() = systemOff || level == "off"

    fun read(context: Context) {
        systemOff = Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    }

    fun setLevel(l: String) {
        level = if (l in listOf("full", "reduced", "off")) l else "full"
    }

    private fun ms(full: Int): Int = when {
        off -> 0
        level == "reduced" -> REDUCED[0].coerceAtMost(full)
        else -> full
    }

    fun <T> spring(ms: Int = DUR, delay: Int = 0): FiniteAnimationSpec<T> =
        if (off) snap() else tween(ms(ms), if (reduced) 0 else delay, Spring)

    fun <T> pop(ms: Int = DUR, delay: Int = 0): FiniteAnimationSpec<T> =
        if (off) snap() else tween(ms(ms), if (reduced) 0 else delay, if (reduced) Spring else Pop)

    fun <T> exit(ms: Int = 180): FiniteAnimationSpec<T> =
        if (off) snap() else tween(ms(ms), 0, Accelerate)
}
