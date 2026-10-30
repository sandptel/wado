package io.github.sandptel.wado.nav

import androidx.navigation3.runtime.NavKey
import kotlinx.serialization.Serializable

/** The pages of "On this phone". */
@Serializable
enum class PhonePage(val title: String) {
    Appearance("Appearance"),
    Background("Notifications & background"),
    Diagnostics("Diagnostics"),
    About("About"),
}

/**
 * Where the app can be: one back stack of these, saved with the activity, so process death and a
 * resume come back to the page the person was on (WebView defect 6). The session is never pushed
 * by anything but a person's pick.
 */
@Serializable
sealed interface Dest : NavKey {
    @Serializable data object Home : Dest

    /** Scan a computer's code. */
    @Serializable data object Pair : Dest

    /** Type the code a computer shows (relay, Remote ID, XXXX-XXXX). */
    @Serializable data object PairTyped : Dest

    @Serializable data class Phone(val page: PhonePage) : Dest

    /** The bundled web client, at a computer (its web key) and a door, until each is native. */
    @Serializable data class Web(val key: String? = null, val door: String? = null) : Dest

    /** The native trackpad on a computer (its web key). */
    @Serializable data class Pad(val key: String) : Dest
}
