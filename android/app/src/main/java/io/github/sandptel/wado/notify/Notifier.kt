package io.github.sandptel.wado.notify

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import io.github.sandptel.wado.MainActivity
import io.github.sandptel.wado.R
import org.json.JSONObject

/**
 * The page's notifications as the phone's own, while wado is not in front (in front, the page
 * shows them itself). One channel per kind of urgency, so a person can silence "computer came
 * online" without silencing "Claude needs permission".
 */
class Notifier(private val context: Context) {
    data class Channel(val id: String, val name: Int, val importance: Int)

    fun createChannels() {
        if (Build.VERSION.SDK_INT < 26) return
        val nm = context.getSystemService(NotificationManager::class.java)
        CHANNELS.forEach { c ->
            nm.createNotificationChannel(NotificationChannel(c.id, context.getString(c.name), c.importance))
        }
    }

    /** `o` is the page's `notify` message: {kind, id, title, body, actions: [{id, label}]}. */
    fun show(o: JSONObject, foreground: Boolean) {
        val kind = o.optString("kind")
        // In front, the page has shown it; a permission prompt is the exception — it must be
        // answerable from the shade even when the page is behind another of wado's own screens.
        if (foreground && kind != "claude_permission") return
        if (!allowed()) return
        val id = o.optString("id")
        val n = NotificationCompat.Builder(context, channelFor(kind))
            .setSmallIcon(R.drawable.ic_stat_wado)
            .setContentTitle(o.optString("title").ifBlank { context.getString(R.string.app_name) })
            .setContentText(o.optString("body"))
            .setStyle(NotificationCompat.BigTextStyle().bigText(o.optString("body")))
            .setContentIntent(openApp())
            .setAutoCancel(true)
            .setCategory(if (kind == "claude_permission") NotificationCompat.CATEGORY_REMINDER else NotificationCompat.CATEGORY_STATUS)
        val actions = o.optJSONArray("actions")
        if (actions != null) {
            for (i in 0 until minOf(actions.length(), 3)) {
                val a = actions.optJSONObject(i) ?: continue
                n.addAction(0, a.optString("label"), ActionReceiver.intent(context, kind, id, a.optString("id")))
            }
        }
        NotificationManagerCompat.from(context).notify(tag(kind, id), 0, n.build())
    }

    fun clear(kind: String, id: String) = NotificationManagerCompat.from(context).cancel(tag(kind, id), 0)

    private fun allowed() = Build.VERSION.SDK_INT < 33 ||
        ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    private fun openApp(): PendingIntent = PendingIntent.getActivity(
        context, 0,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    companion object {
        const val CH_ASK = "ask"
        const val CH_DONE = "done"
        const val CH_STATUS = "status"
        const val CH_APPS = "apps"
        const val CH_STAY = "stay"
        const val CH_MEDIA = "media"

        val CHANNELS = listOf(
            Channel(CH_ASK, R.string.ch_ask, NotificationManager.IMPORTANCE_HIGH),
            Channel(CH_DONE, R.string.ch_done, NotificationManager.IMPORTANCE_DEFAULT),
            Channel(CH_APPS, R.string.ch_apps, NotificationManager.IMPORTANCE_DEFAULT),
            Channel(CH_STATUS, R.string.ch_status, NotificationManager.IMPORTANCE_LOW),
            Channel(CH_MEDIA, R.string.ch_media, NotificationManager.IMPORTANCE_LOW),
            Channel(CH_STAY, R.string.ch_stay, NotificationManager.IMPORTANCE_MIN),
        )

        /** Which channel a page notification kind goes to. */
        fun channelFor(kind: String) = when (kind) {
            "claude_permission" -> CH_ASK
            "claude_done", "transfer_done" -> CH_DONE
            "computer_online", "computer_offline" -> CH_STATUS
            else -> CH_APPS
        }

        fun tag(kind: String, id: String) = "$kind:$id"
    }
}
