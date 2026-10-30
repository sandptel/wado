package io.github.sandptel.wado.clip

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.PersistableBundle
import io.github.sandptel.wado.bridge.BridgeProtocol
import io.github.sandptel.wado.bridge.Hub
import io.github.sandptel.wado.prefs.Prefs

/**
 * The clipboard between this phone and the computer.
 *
 * - Computer → phone: what the computer copies lands on the phone's clipboard at once (focused or
 *   not), marked sensitive when the computer says it is, so Android hides its preview.
 * - Phone → computer: Android lets an app read the clipboard only while it has focus, so the
 *   phone's copy goes over when wado comes to the front. Never a copy marked sensitive (password
 *   managers mark theirs), and never one this app put there itself.
 */
class ClipSync(private val context: Context, private val prefs: Prefs) {
    private val cm get() = context.getSystemService(ClipboardManager::class.java)
    private var lastSet = ""
    private var lastSent = ""

    fun fromComputer(text: String, sensitive: Boolean) {
        if (!prefs.clipFromComputer || text.isEmpty()) return
        val clip = ClipData.newPlainText(LABEL, text)
        if (sensitive) {
            clip.description.extras = PersistableBundle().apply { putBoolean(EXTRA_IS_SENSITIVE, true) }
        }
        lastSet = text
        runCatching { cm.setPrimaryClip(clip) }
    }

    /** Call when the app's window gains focus. */
    fun toComputer() {
        if (!prefs.clipToComputer) return
        val clip = runCatching { cm.primaryClip }.getOrNull() ?: return
        val desc = clip.description
        if (isSensitive(desc) || desc.label == LABEL) return
        val text = (0 until clip.itemCount).firstNotNullOfOrNull { clip.getItemAt(it).text?.toString() } ?: return
        if (text.isBlank() || text == lastSet || text == lastSent) return
        lastSent = text
        Hub.send(BridgeProtocol.CLIP) { put("text", text) }
    }

    companion object {
        const val LABEL = "wado"
        /** ClipDescription.EXTRA_IS_SENSITIVE (API 33); the same key works before as plain data. */
        val EXTRA_IS_SENSITIVE: String =
            if (Build.VERSION.SDK_INT >= 33) ClipDescription.EXTRA_IS_SENSITIVE else "android.content.extra.IS_SENSITIVE"

        fun isSensitive(desc: ClipDescription): Boolean =
            desc.extras?.getBoolean(EXTRA_IS_SENSITIVE, false) == true
    }
}
