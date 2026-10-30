package io.github.sandptel.wado.bridge

import android.os.Handler
import android.os.Looper
import androidx.webkit.JavaScriptReplyProxy
import io.github.sandptel.wado.prefs.Prefs
import io.github.sandptel.wado.ui.theme.Base16
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import org.json.JSONObject

/**
 * The one meeting point of the page and the app. The page's messages land here and are handed
 * to whoever owns that kind ([Handlers]); the app's messages go out through the page's reply
 * proxy, queued until the page has spoken once (it does at load: theme, computers).
 *
 * Everything runs on the main thread: the WebView calls in on it, and posts must leave from it.
 */
object Hub {
    /** Who handles the page's messages that are not plain state. Set by WadoApp / MainActivity. */
    interface Handlers {
        fun haptic(kind: String) {}
        fun active(on: Boolean) {}
        fun orient(lock: String) {}
        fun clip(text: String, sensitive: Boolean) {}
        fun notify(o: JSONObject) {}
        fun notifyClear(kind: String, id: String) {}
        fun pushSubscribe(vapid: String, rid: String) {}
        fun nowPlaying(np: NowPlaying) {}
        /** No activity to show a prompt on: answer at once, never leave the page waiting. */
        fun webauthn(o: JSONObject) {
            send(BridgeProtocol.WEBAUTHN_RESULT) {
                put("id", o.optInt("id")); put("ok", false); put("errName", "NotAllowedError"); put("errCode", "busy")
                put("errMessage", "Open wado on the phone's screen to use your passkey, then try again.")
            }
        }
        fun webauthnCancel(id: Int) {}
    }

    private val main = Handler(Looper.getMainLooper())
    private var proxy: JavaScriptReplyProxy? = null
    private val queue = ArrayDeque<String>()
    private lateinit var prefs: Prefs
    var app: Handlers = object : Handlers {}
    var screen: Handlers = object : Handlers {}

    private val _computers = MutableStateFlow(Computers())
    val computers: StateFlow<Computers> = _computers.asStateFlow()
    private val _theme = MutableStateFlow(Base16.DEFAULT)
    val theme: StateFlow<Base16> = _theme.asStateFlow()
    private val _nowPlaying = MutableStateFlow(NowPlaying())
    val nowPlaying: StateFlow<NowPlaying> = _nowPlaying.asStateFlow()
    private val _pageReady = MutableStateFlow(false)
    /** The page has loaded and spoken: messages to it are delivered now, not queued. */
    val pageReady: StateFlow<Boolean> = _pageReady.asStateFlow()

    private val _events = MutableSharedFlow<Pair<String, JSONObject>>(extraBufferCapacity = 64)
    /** One-off page events the screens react to: `exit`, `shareDone`, `padState`, `padMsg`. */
    val events: SharedFlow<Pair<String, JSONObject>> = _events.asSharedFlow()

    fun init(prefs: Prefs) {
        this.prefs = prefs
        _computers.value = prefs.computers
        _theme.value = prefs.theme
    }

    /** A page message, from the WebView's listener (origin already checked). */
    fun receive(raw: String?, reply: JavaScriptReplyProxy) {
        if (proxy !== reply) {
            proxy = reply
            _pageReady.value = true
            while (queue.isNotEmpty()) reply.postMessage(queue.removeFirst())
        }
        val (t, o) = BridgeProtocol.parse(raw) ?: return
        when (t) {
            BridgeProtocol.THEME -> {
                val arr = o.optJSONArray("hexes")
                val scheme = Base16.parse(arr?.let { a -> (0 until a.length()).map(a::optString) })
                if (scheme != null && scheme.slots != _theme.value.slots) {
                    _theme.value = scheme
                    prefs.theme = scheme
                }
            }
            BridgeProtocol.COMPUTERS -> {
                val c = Computers.parse(o)
                if (c != _computers.value) {
                    _computers.value = c
                    prefs.computers = c
                }
            }
            BridgeProtocol.NOW_PLAYING -> {
                val np = NowPlaying.parse(o)
                _nowPlaying.value = np
                app.nowPlaying(np)
            }
            BridgeProtocol.HAPTIC -> screen.haptic(o.optString("kind"))
            BridgeProtocol.ACTIVE -> screen.active(o.optBoolean("on"))
            BridgeProtocol.ORIENT -> screen.orient(o.optString("lock"))
            BridgeProtocol.WEBAUTHN -> screen.webauthn(o)
            BridgeProtocol.WEBAUTHN_CANCEL -> screen.webauthnCancel(o.optInt("id"))
            BridgeProtocol.CLIP -> app.clip(o.optString("text"), o.optBoolean("sensitive"))
            BridgeProtocol.NOTIFY -> app.notify(o)
            BridgeProtocol.NOTIFY_CLEAR -> app.notifyClear(o.optString("kind"), o.optString("id"))
            BridgeProtocol.PUSH_SUBSCRIBE -> app.pushSubscribe(o.optString("vapid"), o.optString("rid"))
            else -> _events.tryEmit(t to o)
        }
    }

    /** The page went away (reload, navigation): queue until the new one speaks. */
    fun pageGone() {
        proxy = null
        _pageReady.value = false
    }

    /** The phone's font scale, for the page's "Follow phone" text size. */
    fun fontScale(scale: Float) = send(BridgeProtocol.FONT_SCALE) { put("scale", scale.toDouble()) }

    /** Send a message to the page, from any thread. */
    fun send(t: String, build: JSONObject.() -> Unit = {}) {
        val msg = BridgeProtocol.message(t, build)
        main.post {
            val p = proxy
            if (p != null) p.postMessage(msg) else {
                queue.addLast(msg)
                while (queue.size > 32) queue.removeFirst()
            }
        }
    }
}
