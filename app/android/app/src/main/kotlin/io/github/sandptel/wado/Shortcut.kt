package io.github.sandptel.wado

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import java.net.URLEncoder

/**
 * The one dynamic shortcut: the desktop on the last used computer, by its name ("Desktop on
 * studio"). The three static ones (res/xml/shortcuts.xml) follow the last used computer too.
 */
object Shortcut {
    fun desktop(context: Context, key: String, title: String) {
        if (key.isEmpty()) {
            ShortcutManagerCompat.removeDynamicShortcuts(context, listOf("desktop"))
            return
        }
        val link = "wado://go/session?key=" + URLEncoder.encode(key, "UTF-8")
        val info = ShortcutInfoCompat.Builder(context, "desktop")
            .setShortLabel(context.getString(R.string.sc_session))
            .setLongLabel(context.getString(R.string.sc_session_on, title))
            .setIcon(IconCompat.createWithResource(context, R.drawable.sc_session))
            .setIntent(Intent(Intent.ACTION_VIEW, Uri.parse(link)).setClass(context, MainActivity::class.java))
            .build()
        runCatching { ShortcutManagerCompat.pushDynamicShortcut(context, info) }
    }
}
