package io.github.sandptel.wado.push

import android.content.Context
import org.json.JSONObject
import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.PushService
import org.unifiedpush.android.connector.UnifiedPush
import org.unifiedpush.android.connector.data.PushEndpoint
import org.unifiedpush.android.connector.data.PushMessage
import io.github.sandptel.wado.notify.Notifier

/**
 * UnifiedPush, the Google-free push (from the native app's push/): one registration per computer —
 * the instance is its Remote ID, keyed by its VAPID public key — so every computer can reach this
 * phone. The endpoint and its keys are a Web Push subscription in all but name, so the computer's
 * sender (RFC 8291 + VAPID) needs nothing new. With no distributor installed Dart is told plainly.
 */
object Push {
    /** Dart's side: {rid, endpoint, keys:{p256dh, auth}} or {error}. Set by the activity. */
    var answer: ((Map<String, Any?>) -> Unit)? = null

    fun subscribe(context: Context, vapid: String, rid: String) {
        if (UnifiedPush.getDistributors(context).isEmpty()) {
            return fail("No UnifiedPush app on this phone — install one (ntfy, for one), or turn on Stay connected.")
        }
        UnifiedPush.tryUseCurrentOrDefaultDistributor(context) { ok ->
            if (!ok) return@tryUseCurrentOrDefaultDistributor fail("No UnifiedPush app was chosen.")
            runCatching {
                UnifiedPush.register(context, rid.ifBlank { "default" }, "wado notifications", vapid.ifBlank { null })
            }.onFailure { e -> fail(e.message ?: "Push registration failed.") }
        }
    }

    fun fail(why: String) {
        answer?.invoke(mapOf("error" to why))
    }
}

class PushServiceImpl : PushService() {
    override fun onNewEndpoint(endpoint: PushEndpoint, instance: String) {
        val keys = endpoint.pubKeySet
        Push.answer?.invoke(mapOf(
            "rid" to instance, "endpoint" to endpoint.url,
            "keys" to mapOf("p256dh" to (keys?.pubKey ?: ""), "auth" to (keys?.auth ?: "")),
        ))
    }

    /** A `wado_protocol::push::PushNote`, the same one the web app's service worker renders. */
    override fun onMessage(message: PushMessage, instance: String) {
        val o = runCatching { JSONObject(String(message.content, Charsets.UTF_8)) }.getOrNull() ?: return
        val session = o.optString("session")
        val ask = o.optString("ask")
        val id = o.optString("tag").ifBlank { if (ask.isNotBlank()) "$session:$ask" else o.optLong("at").toString() }
        val name = o.optString("computer_name")
        val title = o.optString("title")
        val actions = o.optJSONArray("actions")?.let { a ->
            (0 until a.length()).mapNotNull { i ->
                a.optJSONObject(i)?.let { x -> mapOf("id" to x.optString("id"), "label" to x.optString("title").ifBlank { x.optString("label") }) }
            }
        } ?: emptyList()
        Notifier(this).apply { createChannels() }.show(
            o.optString("kind"), id, if (name.isNotBlank() && !title.contains(name)) "$title · $name" else title, o.optString("body"), actions,
        )
    }

    override fun onRegistrationFailed(reason: FailedReason, instance: String) {
        Push.fail("push registration failed: $reason")
    }

    override fun onUnregistered(instance: String) {}
}
