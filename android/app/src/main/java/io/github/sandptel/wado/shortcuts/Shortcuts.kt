package io.github.sandptel.wado.shortcuts

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import io.github.sandptel.wado.MainActivity
import io.github.sandptel.wado.R
import io.github.sandptel.wado.bridge.Computers
import io.github.sandptel.wado.links.DeepLinks

/**
 * The one dynamic shortcut: the desktop on the last used computer, by its name ("Desktop on
 * studio"). The three static ones (res/xml/shortcuts.xml) follow the last used computer too.
 */
object Shortcuts {
    private const val ID = "desktop"

    fun update(context: Context, computers: Computers) {
        val c = computers.current
        if (c == null) {
            ShortcutManagerCompat.removeDynamicShortcuts(context, listOf(ID))
            return
        }
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(DeepLinks.doorLink("session", c.key)))
            .setClass(context, MainActivity::class.java)
        val info = ShortcutInfoCompat.Builder(context, ID)
            .setShortLabel(context.getString(R.string.sc_session))
            .setLongLabel(context.getString(R.string.sc_session_on, c.title))
            .setIcon(IconCompat.createWithResource(context, R.drawable.sc_session))
            .setIntent(intent)
            .build()
        runCatching { ShortcutManagerCompat.pushDynamicShortcut(context, info) }
    }
}
