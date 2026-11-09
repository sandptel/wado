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
    /** A share-target shortcut's id: this, then the computer's key. */
    const val TO = "to:"
    private const val SHARE = "io.github.sandptel.wado.SHARE_TO_COMPUTER"

    /**
     * The paired computers as share targets — the share sheet's top row, where people are —
     * replacing the last set. `computers`: (key, name), most used first.
     */
    fun shareTargets(context: Context, computers: List<Pair<String, String>>) {
        val keep = computers.take(4)
        val stale = ShortcutManagerCompat.getDynamicShortcuts(context)
            .map { it.id }
            .filter { it.startsWith(TO) && keep.none { (k, _) -> TO + k == it } }
        runCatching { ShortcutManagerCompat.removeDynamicShortcuts(context, stale) }
        for ((key, name) in keep) {
            val info = ShortcutInfoCompat.Builder(context, TO + key)
                .setShortLabel(name)
                .setLongLabel(context.getString(R.string.share_to, name))
                .setIcon(IconCompat.createWithResource(context, R.drawable.sc_session))
                .setCategories(setOf(SHARE))
                .setLongLived(true)
                .setIntent(Intent(Intent.ACTION_SEND).setType("*/*").setClass(context, MainActivity::class.java))
                .build()
            runCatching { ShortcutManagerCompat.pushDynamicShortcut(context, info) }
        }
    }

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
