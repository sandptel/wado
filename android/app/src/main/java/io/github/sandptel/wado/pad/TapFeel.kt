package io.github.sandptel.wado.pad

import androidx.compose.ui.geometry.Offset

/**
 * When to tick in the hand: one finger down and up quickly without travelling — what the
 * computer's libinput will read as a click. The click itself is the computer's; this only
 * mirrors it (the same rule as the web pad, js/native_feel.js). Pure, so it is unit-tested.
 */
class TapFeel(private val maxMs: Long = 180, private val maxPx: Float = 24f) {
    private data class Down(val at: Offset, val t: Long, val alone: Boolean)

    private val downs = HashMap<Long, Down>()

    fun down(id: Long, at: Offset, t: Long, fingers: Int) {
        // A second finger makes every finger down part of a multi-finger touch.
        if (fingers > 1) downs.replaceAll { _, d -> d.copy(alone = false) }
        downs[id] = Down(at, t, fingers == 1)
    }

    /** True when this lift was a tap. `none`: no finger is left down. */
    fun up(id: Long, at: Offset, t: Long, none: Boolean): Boolean {
        val d = downs.remove(id) ?: return false
        return d.alone && none && t - d.t < maxMs && (at - d.at).getDistance() < maxPx
    }
}
