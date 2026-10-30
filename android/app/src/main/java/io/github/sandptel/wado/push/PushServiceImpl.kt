package io.github.sandptel.wado.push

import io.github.sandptel.wado.WadoApp
import io.github.sandptel.wado.bridge.BridgeProtocol
import io.github.sandptel.wado.bridge.Hub
import org.json.JSONObject
import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.PushService
import org.unifiedpush.android.connector.data.PushEndpoint
import org.unifiedpush.android.connector.data.PushMessage

/**
 * UnifiedPush, the Google-free push: the phone's distributor (ntfy, NextPush, …) wakes this
 * service. The endpoint and its keys are a Web Push subscription in all but name, so the
 * computer's Web Push sender (RFC 8291 aes128gcm + VAPID) reaches it unchanged; the connector
 * decrypts. A message is the same JSON a web push carries (`PushNote`). The page registers the
 * endpoint with the computer as a `PushSub` of kind `unifiedpush` (js/push.js).
 */
class PushServiceImpl : PushService() {
    override fun onNewEndpoint(endpoint: PushEndpoint, instance: String) {
        val keys = endpoint.pubKeySet
        Hub.send(BridgeProtocol.PUSH_ENDPOINT) {
            put("rid", instance)
            put("endpoint", endpoint.url)
            put("keys", JSONObject().put("p256dh", keys?.pubKey ?: "").put("auth", keys?.auth ?: ""))
        }
    }

    override fun onMessage(message: PushMessage, instance: String) {
        val o = runCatching { JSONObject(String(message.content, Charsets.UTF_8)) }.getOrNull() ?: return
        // A `wado_protocol::push::PushNote`, the same one the web app's service worker renders.
        (application as WadoApp).notifier.showPush(o)
    }

    override fun onRegistrationFailed(reason: FailedReason, instance: String) {
        Hub.send(BridgeProtocol.PUSH_ENDPOINT) { put("error", "push registration failed: $reason") }
    }

    override fun onUnregistered(instance: String) {}
}
