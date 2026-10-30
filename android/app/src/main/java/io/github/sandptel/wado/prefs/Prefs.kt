package io.github.sandptel.wado.prefs

import android.content.Context
import android.content.SharedPreferences
import androidx.core.content.edit
import io.github.sandptel.wado.bridge.Computers
import io.github.sandptel.wado.ui.theme.Base16
import org.json.JSONArray
import org.json.JSONObject

/**
 * What the app keeps between launches: its own switches, and a copy of what the client last
 * told it (computers, scheme) so its home draws at once on a cold start, before the page loads.
 * The client's own settings stay in the page's storage; nothing secret is copied here.
 */
class Prefs(context: Context) {
    private val sp: SharedPreferences = context.getSharedPreferences("wado", Context.MODE_PRIVATE)

    var computers: Computers
        get() = Computers.parse(sp.getString(K_COMPUTERS, null)?.let { runCatching { JSONObject(it) }.getOrNull() })
        set(v) = sp.edit { putString(K_COMPUTERS, v.toJson().toString()) }

    var theme: Base16
        get() = sp.getString(K_THEME, null)?.let { s ->
            runCatching { JSONArray(s) }.getOrNull()?.let { a -> Base16.parse((0 until a.length()).map(a::getString)) }
        } ?: Base16.DEFAULT
        set(v) = sp.edit {
            putString(K_THEME, JSONArray(v.slots.map { "%06x".format(it and 0xFFFFFF) }).toString())
        }

    /** "Stay connected": the foreground service holding the control link. Off by default. */
    var stayConnected: Boolean
        get() = sp.getBoolean(K_STAY, false)
        set(v) = sp.edit { putBoolean(K_STAY, v) }

    /** Send the phone's clipboard to the computer when wado comes to the front. */
    var clipToComputer: Boolean
        get() = sp.getBoolean(K_CLIP_OUT, true)
        set(v) = sp.edit { putBoolean(K_CLIP_OUT, v) }

    /** Put what the computer copies on the phone's clipboard. */
    var clipFromComputer: Boolean
        get() = sp.getBoolean(K_CLIP_IN, true)
        set(v) = sp.edit { putBoolean(K_CLIP_IN, v) }

    /** Phone volume keys drive the computer's volume while wado is in front. */
    var volumeKeys: Boolean
        get() = sp.getBoolean(K_VOLUME, true)
        set(v) = sp.edit { putBoolean(K_VOLUME, v) }

    // ── Appearance (local first; ui/phone/Look.kt) ──────────────────────────────────────────

    /** A scheme from schemes.json by name; "" follows the web part's. */
    var scheme: String
        get() = sp.getString(K_SCHEME, "").orEmpty()
        set(v) = sp.edit { putString(K_SCHEME, v) }

    /** Accent slot 0..7 (base08..base0F); -1 is the scheme's own (base0D). */
    var accent: Int
        get() = sp.getInt(K_ACCENT, -1)
        set(v) = sp.edit { putInt(K_ACCENT, v) }

    /** `soft`, `round` or `square`. */
    var corners: String
        get() = sp.getString(K_CORNERS, "soft").orEmpty()
        set(v) = sp.edit { putString(K_CORNERS, v) }

    /** `full`, `reduced` or `off` (AND-ed with the system's animator scale). */
    var motion: String
        get() = sp.getString(K_MOTION, "full").orEmpty()
        set(v) = sp.edit { putString(K_MOTION, v) }

    /** Text size 0.85..1.5; 0 follows the phone's. */
    var textSize: Float
        get() = sp.getFloat(K_TEXT, 0f)
        set(v) = sp.edit { putFloat(K_TEXT, v) }

    /** Interface size: 0.9 compact, 1 cozy, 1.1 comfy, 1.2 large. */
    var uiSize: Float
        get() = sp.getFloat(K_UI, 1f)
        set(v) = sp.edit { putFloat(K_UI, v) }

    companion object {
        private const val K_SCHEME = "scheme"
        private const val K_ACCENT = "accent"
        private const val K_CORNERS = "corners"
        private const val K_MOTION = "motion"
        private const val K_TEXT = "text_size"
        private const val K_UI = "ui_size"
        private const val K_COMPUTERS = "computers"
        private const val K_THEME = "theme"
        private const val K_STAY = "stay_connected"
        private const val K_CLIP_OUT = "clip_to_computer"
        private const val K_CLIP_IN = "clip_from_computer"
        private const val K_VOLUME = "volume_keys"
    }
}
