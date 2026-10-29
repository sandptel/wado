package io.github.sandptel.wado.push

import android.content.Context
import io.github.sandptel.wado.bridge.BridgeProtocol
import io.github.sandptel.wado.bridge.Hub
import org.unifiedpush.android.connector.UnifiedPush

/**
 * The page's `pushSubscribe {vapid}`: register with the phone's UnifiedPush distributor, keyed
 * by the computer's VAPID public key. The endpoint arrives later in [PushServiceImpl]. With no
 * distributor installed the page is told plainly — the installed web app in Chrome is then the
 * way to get true push, or "Stay connected" while wado is closed.
 */
object Push {
    private const val INSTANCE = "default"

    fun subscribe(context: Context, vapid: String) {
        if (UnifiedPush.getDistributors(context).isEmpty()) {
            Hub.send(BridgeProtocol.PUSH_ENDPOINT) {
                put("error", "no UnifiedPush distributor on this phone (install ntfy, for one)")
            }
            return
        }
        UnifiedPush.tryUseCurrentOrDefaultDistributor(context) { ok ->
            if (!ok) {
                Hub.send(BridgeProtocol.PUSH_ENDPOINT) { put("error", "no UnifiedPush distributor was chosen") }
                return@tryUseCurrentOrDefaultDistributor
            }
            runCatching {
                UnifiedPush.register(context, INSTANCE, "wado notifications", vapid.ifBlank { null })
            }.onFailure { e ->
                Hub.send(BridgeProtocol.PUSH_ENDPOINT) { put("error", e.message ?: "push registration failed") }
            }
        }
    }
}
