package io.github.sandptel.wado

import android.app.Application
import android.content.MutableContextWrapper
import io.github.sandptel.wado.bridge.Hub
import io.github.sandptel.wado.bridge.NowPlaying
import io.github.sandptel.wado.clip.ClipSync
import io.github.sandptel.wado.core.CoreHost
import io.github.sandptel.wado.ui.phone.LookStore
import io.github.sandptel.wado.media.RemoteMedia
import io.github.sandptel.wado.notify.Notifier
import io.github.sandptel.wado.prefs.Prefs
import io.github.sandptel.wado.push.Push
import io.github.sandptel.wado.stay.StayConnectedService
import io.github.sandptel.wado.web.ClientWebView
import kotlinx.coroutines.flow.map
import org.json.JSONObject

/**
 * The process: its preferences, the app-wide handlers of the page's messages (notifications,
 * clipboard, now playing, push), and the one WebView, which outlives activities so "Stay
 * connected" can keep the page — and its link to the computer — running with no screen up.
 */
class WadoApp : Application() {
    lateinit var prefs: Prefs
        private set
    lateinit var notifier: Notifier
        private set
    private lateinit var clip: ClipSync
    /** The Rust client core: native pairing, the computers it knows, their live links. */
    lateinit var core: CoreHost
        private set
    private val media by lazy { RemoteMedia(this) }

    /** An activity of ours is in front. Notifications defer to the page then. */
    var foreground = false

    private val webContext by lazy { MutableContextWrapper(this) }
    private var webView: ClientWebView? = null

    override fun onCreate() {
        super.onCreate()
        prefs = Prefs(this)
        notifier = Notifier(this).also { it.createChannels() }
        clip = ClipSync(this, prefs)
        Hub.init(prefs)
        Hub.app = object : Hub.Handlers {
            override fun clip(text: String, sensitive: Boolean) = clip.fromComputer(text, sensitive)
            override fun notify(o: JSONObject) = notifier.show(o, foreground)
            override fun notifyClear(kind: String, id: String) = notifier.clear(kind, id)
            override fun pushSubscribe(vapid: String, rid: String) = Push.subscribe(this@WadoApp, vapid, rid)
            override fun nowPlaying(np: NowPlaying) = media.update(np)
        }
        LookStore.load(prefs)
        core = CoreHost(this).also { it.start(Hub.computers.map { c -> c.list }) }
        if (prefs.stayConnected) StayConnectedService.sync(this, true)
    }

    /** The WebView, created on first use and loaded with the client (at `firstQuery`, if made now). */
    fun web(firstQuery: String = ""): ClientWebView = webView ?: ClientWebView(webContext).also {
        webView = it
        it.loadHome(firstQuery)
    }

    /** Was the WebView already made (the page may already be up)? */
    val hasWeb get() = webView != null

    /** Dialogs the page opens (a `<select>`) need an activity's context; the app's otherwise. */
    fun webContextFor(activity: android.app.Activity?) {
        webContext.baseContext = activity ?: this
    }

    /** The phone's clipboard to the computer — when a window of ours has just gained focus. */
    fun clipToComputer() = clip.toComputer()
}
