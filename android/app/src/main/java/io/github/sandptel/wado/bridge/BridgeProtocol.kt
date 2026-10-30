package io.github.sandptel.wado.bridge

import org.json.JSONObject

/**
 * The `window.wadoNative` protocol: JSON objects with a `t`, both ways. The page's side is
 * crates/client/src/js/native*.js; the list is in docs/android.md. Keep the three in step.
 */
object BridgeProtocol {
    /** The JS object name the WebView injects (WebViewCompat.addWebMessageListener). */
    const val OBJECT = "wadoNative"

    // page → app
    const val THEME = "theme"
    const val COMPUTERS = "computers"
    const val NOW_PLAYING = "nowPlaying"
    const val CLIP = "clip"
    const val HAPTIC = "haptic"
    const val ACTIVE = "active"
    const val ORIENT = "orient"
    const val NOTIFY = "notify"
    const val NOTIFY_CLEAR = "notifyClear"
    const val PUSH_SUBSCRIBE = "pushSubscribe"
    const val SHARE_DONE = "shareDone"
    const val EXIT = "exit"
    const val PAD_STATE = "padState"
    const val PAD_MSG = "padMsg"

    // app → page
    const val BACK = "back"
    const val GO = "go"
    const val MEDIA = "media"
    const val VOLUME = "volume"
    const val SHARE = "share"
    const val NOTIFY_ACTION = "notifyAction"
    const val PUSH_ENDPOINT = "pushEndpoint"
    const val PAD_START = "padStart"
    const val PAD = "pad"
    const val PAD_STOP = "padStop"
    // CLIP is both ways: page → app the computer's copy, app → page the phone's.

    fun message(t: String, build: JSONObject.() -> Unit = {}): String =
        JSONObject().put("t", t).apply(build).toString()

    /** The type of an incoming message, or null if it is not one of ours. */
    fun parse(raw: String?): Pair<String, JSONObject>? {
        val o = try {
            JSONObject(raw ?: return null)
        } catch (_: Exception) {
            return null
        }
        val t = o.optString("t")
        return if (t.isEmpty()) null else t to o
    }

    /** The document-start script: what the page can read synchronously about its host. */
    fun infoScript(version: String, sdk: Int): String =
        "window.wadoNativeInfo=" + JSONObject().put("platform", "android").put("version", version)
            .put("sdk", sdk).toString() + ";"
}
