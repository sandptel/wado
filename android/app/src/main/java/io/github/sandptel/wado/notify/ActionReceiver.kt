package io.github.sandptel.wado.notify

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationManagerCompat
import io.github.sandptel.wado.bridge.BridgeProtocol
import io.github.sandptel.wado.bridge.Hub

/**
 * A notification's button (Allow / Deny on a Claude permission prompt): handed to the page,
 * which answers the computer over its link. That needs the page alive — the app open, or kept
 * by "Stay connected"; otherwise the computer's own prompt still waits for an answer.
 */
class ActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val kind = intent.getStringExtra(EXTRA_KIND) ?: return
        val id = intent.getStringExtra(EXTRA_ID) ?: return
        val action = intent.getStringExtra(EXTRA_ACTION) ?: return
        Hub.send(BridgeProtocol.NOTIFY_ACTION) {
            put("kind", kind)
            put("id", id)
            put("action", action)
        }
        NotificationManagerCompat.from(context).cancel(Notifier.tag(kind, id), 0)
    }

    companion object {
        private const val EXTRA_KIND = "kind"
        private const val EXTRA_ID = "id"
        private const val EXTRA_ACTION = "action"

        fun intent(context: Context, kind: String, id: String, action: String): PendingIntent =
            PendingIntent.getBroadcast(
                context,
                "$kind:$id:$action".hashCode(),
                Intent(context, ActionReceiver::class.java)
                    .putExtra(EXTRA_KIND, kind).putExtra(EXTRA_ID, id).putExtra(EXTRA_ACTION, action),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
    }
}
