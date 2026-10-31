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

/**
 * wado's notifications as the phone's own (from the native app's Notifier): a session app's
 * notification, Claude asking, a transfer finished — shown while wado is not in front. One channel
 * per kind of urgency, so "computer is back" can be silenced without silencing "Claude needs
 * permission". A tap or a button opens wado with what was pressed (`notifyAction` to Dart).
 */
class Notifier(private val context: Context) {
    fun createChannels() {
        if (Build.VERSION.SDK_INT < 26) return
        val nm = context.getSystemService(NotificationManager::class.java)
        listOf(
            Triple(CH_ASK, R.string.ch_ask, NotificationManager.IMPORTANCE_HIGH),
            Triple(CH_DONE, R.string.ch_done, NotificationManager.IMPORTANCE_DEFAULT),
            Triple(CH_APPS, R.string.ch_apps, NotificationManager.IMPORTANCE_DEFAULT),
            Triple(CH_STATUS, R.string.ch_status, NotificationManager.IMPORTANCE_LOW),
            Triple(CH_MEDIA, R.string.ch_media, NotificationManager.IMPORTANCE_LOW),
            Triple(CH_STAY, R.string.ch_stay, NotificationManager.IMPORTANCE_MIN),
        ).forEach { (id, name, imp) -> nm.createNotificationChannel(NotificationChannel(id, context.getString(name), imp)) }
    }

    /** {kind, id, title, body, actions: [{id, label}]} */
    fun show(kind: String, id: String, title: String, body: String, actions: List<Map<String, String>>) {
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return
        val b = NotificationCompat.Builder(context, channelFor(kind))
            .setSmallIcon(R.drawable.ic_stat_wado)
            .setContentTitle(title.ifBlank { "wado" })
            .setContentText(body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setContentIntent(open(kind, id, ""))
            .setAutoCancel(true)
            .setCategory(if (kind == KIND_ASK) NotificationCompat.CATEGORY_REMINDER else NotificationCompat.CATEGORY_STATUS)
        actions.take(2).forEach { a -> b.addAction(0, a["label"] ?: "", open(kind, id, a["id"] ?: "")) }
        NotificationManagerCompat.from(context).notify("$kind:$id", 0, b.build())
    }

    fun clear(kind: String, id: String) = NotificationManagerCompat.from(context).cancel("$kind:$id", 0)

    private fun open(kind: String, id: String, action: String): PendingIntent = PendingIntent.getActivity(
        context,
        "$kind:$id:$action".hashCode(),
        Intent(context, MainActivity::class.java)
            .setAction(ACTION)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra("kind", kind).putExtra("id", id).putExtra("action", action),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    companion object {
        const val ACTION = "io.github.sandptel.wado.NOTIFY_ACTION"
        const val KIND_ASK = "claude_permission"
        const val CH_ASK = "ask"
        const val CH_DONE = "done"
        const val CH_STATUS = "status"
        const val CH_APPS = "apps"
        const val CH_STAY = "stay"
        const val CH_MEDIA = "media"

        fun channelFor(kind: String) = when (kind) {
            KIND_ASK -> CH_ASK
            "claude_done", "transfer", "transfer_done" -> CH_DONE
            "online", "computer_online", "computer_offline" -> CH_STATUS
            else -> CH_APPS
        }
    }
}
