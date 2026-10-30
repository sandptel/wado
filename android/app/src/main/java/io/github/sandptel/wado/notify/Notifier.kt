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
 * wado's notifications as the phone's own. Two sources, one shape ([Note]):
 * - the page's `notify` messages, while wado is not in front (in front, the page shows them);
 * - a computer's pushes (`wado_protocol::push::PushNote`, via UnifiedPush), always.
 *
 * One channel per kind of urgency, so a person can silence "computer is back" without silencing
 * "Claude needs permission". A button opens wado with what was pressed ([Action]): Allow is
 * behind the passkey, which needs the app in front.
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

    /** A page `notify` message: {kind, id, title, body, actions: [{id, label}]}. */
    fun show(o: JSONObject, foreground: Boolean) {
        val n = Note.fromPage(o)
        // In front, the page has shown it; a permission prompt is the exception — it must be
        // answerable from the shade even behind another of wado's own screens.
        if (foreground && n.kind != KIND_ASK) return
        post(n)
    }

    /** A pushed `PushNote`. */
    fun showPush(o: JSONObject) = post(Note.fromPush(o))

    fun clear(kind: String, id: String) = NotificationManagerCompat.from(context).cancel(tag(kind, id), 0)

    private fun post(n: Note) {
        if (!allowed()) return
        val b = NotificationCompat.Builder(context, channelFor(n.kind))
            .setSmallIcon(R.drawable.ic_stat_wado)
            .setContentTitle(n.title.ifBlank { context.getString(R.string.app_name) })
            .setContentText(n.body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(n.body))
            .setContentIntent(Action.open(context, n, ""))
            .setAutoCancel(true)
            .setCategory(if (n.kind == KIND_ASK) NotificationCompat.CATEGORY_REMINDER else NotificationCompat.CATEGORY_STATUS)
        n.actions.take(2).forEach { (id, label) -> b.addAction(0, label, Action.open(context, n, id)) }
        NotificationManagerCompat.from(context).notify(tag(n.kind, n.id), 0, b.build())
    }

    private fun allowed() = Build.VERSION.SDK_INT < 33 ||
        ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    companion object {
        const val KIND_ASK = "claude_permission"
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

        /** Which channel a kind goes to — the PushNote kinds (`NOTE_*`) and the page's own. */
        fun channelFor(kind: String) = when (kind) {
            KIND_ASK -> CH_ASK
            "claude_done", "transfer", "transfer_done" -> CH_DONE
            "online", "computer_online", "computer_offline" -> CH_STATUS
            else -> CH_APPS
        }

        fun tag(kind: String, id: String) = "$kind:$id"
    }
}

/** One notification, whichever way it came. Pure, so the mapping is unit-tested. */
data class Note(
    val kind: String,
    val id: String,
    val title: String,
    val body: String,
    val actions: List<Pair<String, String>>,
    val computer: String = "",
    val session: String = "",
    val ask: String = "",
) {
    companion object {
        private fun actions(o: JSONObject) = o.optJSONArray("actions")?.let { a ->
            (0 until a.length()).mapNotNull { i ->
                a.optJSONObject(i)?.let { x -> x.optString("id") to x.optString("title").ifBlank { x.optString("label") } }
            }
        } ?: emptyList()

        fun fromPage(o: JSONObject) = Note(
            kind = o.optString("kind"),
            id = o.optString("id"),
            title = o.optString("title"),
            body = o.optString("body"),
            actions = actions(o),
        )

        /** `wado_protocol::push::PushNote`. A later note with the same tag replaces this one. */
        fun fromPush(o: JSONObject): Note {
            val session = o.optString("session")
            val ask = o.optString("ask")
            val id = o.optString("tag").ifBlank { if (ask.isNotBlank()) "$session:$ask" else o.optLong("at").toString() }
            val name = o.optString("computer_name")
            val title = o.optString("title")
            return Note(
                kind = o.optString("kind"),
                id = id,
                title = if (name.isNotBlank() && !title.contains(name)) "$title · $name" else title,
                body = o.optString("body"),
                actions = actions(o),
                computer = o.optString("computer"),
                session = session,
                ask = ask,
            )
        }
    }
}

/** A notification's tap or button: opens wado and hands the page `notifyAction`. */
object Action {
    const val INTENT = "io.github.sandptel.wado.NOTIFY_ACTION"

    fun open(context: Context, n: Note, action: String): PendingIntent = PendingIntent.getActivity(
        context,
        "${n.kind}:${n.id}:$action".hashCode(),
        Intent(context, MainActivity::class.java)
            .setAction(INTENT)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra("kind", n.kind).putExtra("id", n.id).putExtra("action", action)
            .putExtra("computer", n.computer).putExtra("session", n.session).putExtra("ask", n.ask),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
}
