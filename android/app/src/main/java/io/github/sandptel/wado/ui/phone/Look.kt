package io.github.sandptel.wado.ui.phone

import io.github.sandptel.wado.prefs.Prefs
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/** This phone's appearance: local first (Prefs), the source of truth for the native screens. */
data class Look(
    /** "" follows the web part's scheme. */
    val scheme: String = "",
    val accent: Int = -1,
    val corners: String = "soft",
    val motion: String = "full",
    val size: Size = Size(),
)

object LookStore {
    private val _look = MutableStateFlow(Look())
    val look: StateFlow<Look> = _look

    fun load(prefs: Prefs) {
        val t = prefs.textSize
        _look.value = Look(
            scheme = prefs.scheme,
            accent = prefs.accent,
            corners = prefs.corners,
            motion = prefs.motion,
            size = Size(text = if (t > 0f) t else 1f, follow = t <= 0f, ui = prefs.uiSize),
        )
    }

    fun set(prefs: Prefs, l: Look) {
        prefs.scheme = l.scheme
        prefs.accent = l.accent
        prefs.corners = l.corners
        prefs.motion = l.motion
        prefs.textSize = if (l.size.follow) 0f else l.size.text
        prefs.uiSize = l.size.ui
        _look.value = l
    }
}
