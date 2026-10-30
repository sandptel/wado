package io.github.sandptel.wado.push

import android.content.Context
import io.github.sandptel.wado.bridge.BridgeProtocol
import io.github.sandptel.wado.bridge.Hub
import org.unifiedpush.android.connector.UnifiedPush

/**
 * The page's `pushSubscribe {vapid, rid}`: register with the phone's UnifiedPush distributor
 * for one computer — the instance is its Remote ID, keyed by its VAPID public key — so every
 * computer gets its own endpoint. The endpoint arrives later in [PushServiceImpl]. With no
 * distributor installed the page is told plainly: "Stay connected" is then the way to hear from
 * a computer while wado is closed.
 */
object Push {
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

    private fun fail(why: String) = Hub.send(BridgeProtocol.PUSH_ENDPOINT) { put("error", why) }
}
