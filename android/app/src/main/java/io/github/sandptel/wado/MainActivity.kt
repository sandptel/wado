package io.github.sandptel.wado

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.view.KeyEvent
import android.view.View
import androidx.fragment.app.FragmentActivity
import io.github.sandptel.wado.passkey.Authenticator
import androidx.activity.OnBackPressedCallback
import androidx.activity.SystemBarStyle
import androidx.compose.runtime.LaunchedEffect
import androidx.core.view.WindowCompat
import io.github.sandptel.wado.ui.pad.PadModes
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.core.content.ContextCompat
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.lifecycle.lifecycleScope
import io.github.sandptel.wado.bridge.BridgeProtocol
import io.github.sandptel.wado.bridge.Computer
import io.github.sandptel.wado.bridge.Hub
import io.github.sandptel.wado.device.Haptics
import io.github.sandptel.wado.device.ScreenModes
import io.github.sandptel.wado.links.DeepLinks
import io.github.sandptel.wado.notify.Action
import io.github.sandptel.wado.share.ShareIntent
import io.github.sandptel.wado.share.ShareStore
import io.github.sandptel.wado.shortcuts.Shortcuts
import io.github.sandptel.wado.stay.StayConnectedService
import io.github.sandptel.wado.ui.AppActions
import io.github.sandptel.wado.ui.AppRoot
import io.github.sandptel.wado.ui.AppState
import io.github.sandptel.wado.nav.Dest
import io.github.sandptel.wado.ui.Pages
import io.github.sandptel.wado.ui.share.ShareState
import io.github.sandptel.wado.ui.theme.Motion
import io.github.sandptel.wado.ui.theme.WadoTheme
import io.github.sandptel.wado.web.ClientServer
import io.github.sandptel.wado.web.ClientWebView
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject
import java.net.URI

/**
 * The one activity: native screens (home, pairing, share sheet, settings, about) over the
 * client's WebView, and the routing of everything that opens wado — the launcher, shortcuts,
 * a share, a QR's link, a notification.
 */
class MainActivity : FragmentActivity(), ClientWebView.Host, AppActions {
    private val app get() = application as WadoApp
    private val state = AppState()
    private lateinit var web: ClientWebView
    private lateinit var modes: ScreenModes
    private val authenticator by lazy { Authenticator(this) }
    private var screenHandlers: Hub.Handlers? = null
    private var hudJob: Job? = null

    private var mediaDone: ((Boolean, Boolean) -> Unit)? = null
    private var mediaWanted = Pair(false, false)
    private val askMedia = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { res ->
        val (cam, mic) = mediaWanted
        mediaDone?.invoke(
            cam && (res[Manifest.permission.CAMERA] ?: has(Manifest.permission.CAMERA)),
            mic && (res[Manifest.permission.RECORD_AUDIO] ?: has(Manifest.permission.RECORD_AUDIO)),
        )
        mediaDone = null
    }
    private var filesDone: ((Array<Uri>?) -> Unit)? = null
    private val pickMany = registerForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) {
        filesDone?.invoke(it.toTypedArray().takeIf { a -> a.isNotEmpty() }); filesDone = null
    }
    private val pickOne = registerForActivityResult(ActivityResultContracts.OpenDocument()) {
        filesDone?.invoke(it?.let { u -> arrayOf(u) }); filesDone = null
    }
    private val askNotify = registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

    override fun onCreate(savedInstanceState: Bundle?) {
        installSplashScreen()
        // Light icons over the dark ground from the first frame; a light scheme flips them below.
        enableEdgeToEdge(
            SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        super.onCreate(savedInstanceState)
        Motion.read(this)
        modes = ScreenModes(this)
        app.webContextFor(this)
        val fresh = !app.hasWeb
        web = app.web(if (fresh) firstQuery(intent) else "")
        web.host = this
        // A font size change recreates this activity but not the WebView (it outlives activities),
        // so the page hears the new scale here.
        Hub.fontScale(resources.configuration.fontScale)
        Hub.screen = object : Hub.Handlers {
            override fun haptic(kind: String) = Haptics.perform(window.decorView, kind)
            override fun active(on: Boolean) {
                state.active = on
                modes.active(on)
            }
            override fun orient(lock: String) = modes.orient(lock)
            override fun webauthn(o: JSONObject) = passkey(o)
            override fun webauthnCancel(id: Int) = authenticator.cancel(id)
        }
        screenHandlers = Hub.screen
        lifecycleScope.launch {
            Hub.events.collect { (t, o) -> event(t, o) }
        }
        lifecycleScope.launch {
            Hub.computers.collect { Shortcuts.update(this@MainActivity, it) }
        }
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() = back()
        })
        setContent {
            val scheme by Hub.theme.collectAsState()
            val computers by Hub.computers.collectAsState()
            LaunchedEffect(scheme.light) {
                WindowCompat.getInsetsController(window, window.decorView).apply {
                    isAppearanceLightStatusBars = scheme.light
                    isAppearanceLightNavigationBars = scheme.light
                }
            }
            WadoTheme(scheme) {
                AppRoot(web.view, state, computers, { d -> Pages(d, state.nav, computers, app.prefs, this) }, this)
            }
        }
        // A recreated activity comes back to its saved pages; only a new launch is routed.
        if (savedInstanceState == null) route(intent, firstLoad = fresh)
        if (Build.VERSION.SDK_INT >= 33 && Hub.computers.value.list.isNotEmpty() &&
            !has(Manifest.permission.POST_NOTIFICATIONS)
        ) askNotify.launch(Manifest.permission.POST_NOTIFICATIONS)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        route(intent, firstLoad = false)
    }

    override fun onStart() {
        super.onStart()
        app.foreground = true
    }

    override fun onStop() {
        super.onStop()
        app.foreground = false
    }

    override fun onDestroy() {
        if (web.host === this) web.host = null
        // The page outlives this activity: with no screen, a passkey request is answered at once.
        if (Hub.screen === screenHandlers) Hub.screen = object : Hub.Handlers {}
        app.webContextFor(null)
        super.onDestroy()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) app.clipToComputer()
    }

    /** Volume keys drive the computer's volume while wado is in front, when it said what it is. */
    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        val np = Hub.nowPlaying.value
        if ((keyCode == KeyEvent.KEYCODE_VOLUME_UP || keyCode == KeyEvent.KEYCODE_VOLUME_DOWN) &&
            app.prefs.volumeKeys && np.volume >= 0f && !np.isEmpty
        ) {
            val step = if (keyCode == KeyEvent.KEYCODE_VOLUME_UP) 0.05f else -0.05f
            val level = ((state.volumeHud ?: np.volume) + step).coerceIn(0f, 1f)
            Hub.send(BridgeProtocol.VOLUME) { put("volume", level.toDouble()) }
            Haptics.perform(window.decorView, "tick")
            state.volumeHud = level
            hudJob?.cancel()
            hudJob = lifecycleScope.launch {
                delay(1400)
                state.volumeHud = null
            }
            return true
        }
        return super.onKeyDown(keyCode, event)
    }

    // ── what opened wado ────────────────────────────────────────────────────────────────

    /** On a cold start, a connect link is loaded with the page rather than after it. */
    private fun firstQuery(intent: Intent?): String =
        intent?.dataString?.let(DeepLinks::connect)?.let(DeepLinks::connectQuery) ?: ""

    private fun route(intent: Intent?, firstLoad: Boolean) {
        if (intent == null) return
        if (intent.action == Action.INTENT) {
            // A notification: its body opens the client (at its computer, for a push); a button is
            // handed to the page — queued by the Hub until the page is up, on a cold start too.
            val action = intent.getStringExtra("action").orEmpty()
            val computer = intent.getStringExtra("computer").orEmpty()
            Hub.computers.value.list.firstOrNull { it.remoteId.filter(Char::isDigit) == computer }?.let { c ->
                Hub.send(BridgeProtocol.GO) { put("key", c.key); put("go", "") }
            }
            if (action.isNotEmpty()) {
                Hub.send(BridgeProtocol.NOTIFY_ACTION) {
                    for (k in listOf("kind", "id", "action", "computer", "session", "ask")) put(k, intent.getStringExtra(k).orEmpty())
                }
            }
            state.nav.go(Dest.Web())
            return
        }
        if (ShareIntent.isShare(intent)) {
            val payload = ShareIntent.read(intent, contentResolver)
            if (!payload.isEmpty) {
                state.shareState = ShareState.Choosing
                state.share = payload
            }
            return
        }
        val data = intent.dataString ?: return
        val door = runCatching { DeepLinks.door(URI(data)) }.getOrNull()
        if (door != null) {
            val c = Hub.computers.value.list.firstOrNull { it.key == door.key } ?: Hub.computers.value.current
            if (door.door == "trackpad" && c != null) state.nav.go(Dest.Pad(c.key)) else door(c, door.door)
            return
        }
        DeepLinks.connect(data)?.let { link -> if (firstLoad) state.nav.go(Dest.Web()) else connect(link) }
    }

    private fun event(t: String, o: JSONObject) {
        when (t) {
            BridgeProtocol.EXIT -> state.nav.home()
            BridgeProtocol.SHARE_DONE -> {
                val s = state.shareState
                val to = (s as? ShareState.Sending)?.to ?: return
                state.shareState = if (o.optBoolean("ok")) ShareState.Done(to)
                else ShareState.Failed(to, o.optString("why").ifBlank { "something went wrong" })
                Haptics.perform(window.decorView, if (o.optBoolean("ok")) "confirm" else "reject")
            }
        }
    }

    private fun back() {
        when {
            state.fullscreen != null -> {
                fullscreenExit?.invoke()
                fullscreen(null, null)
            }
            state.share != null -> closeShare()
            state.nav.top is Dest.Web -> webBack()
            !state.nav.pop() -> moveTaskToBack(true)
        }
    }

    // ── AppActions ──────────────────────────────────────────────────────────────────────

    override fun webBack() {
        if (web.view.canGoBack()) web.view.goBack() else Hub.send(BridgeProtocol.BACK)
    }

    override fun exitFullscreen() {
        fullscreenExit?.invoke()
        fullscreen(null, null)
    }

    override val padModes = object : PadModes {
        override fun enter() {
            modes.orient("landscape")
            modes.active(true)
        }

        override fun leave() {
            modes.orient("none")
            modes.active(state.active)
        }
    }

    override fun door(c: Computer?, go: String) {
        Hub.send(BridgeProtocol.GO) { put("key", c?.key ?: ""); put("go", go) }
        state.nav.go(Dest.Web(c?.key, go))
    }

    override fun connect(link: Map<String, String>) {
        web.loadHome(DeepLinks.connectQuery(link))
        state.nav.go(Dest.Web())
    }

    override fun pick(c: Computer) {
        val payload = state.share ?: return
        val token = ShareStore.put(payload.files)
        state.shareState = ShareState.Sending(c)
        Hub.send(BridgeProtocol.SHARE) {
            put("key", c.key)
            put("files", JSONArray().apply {
                payload.files.forEachIndexed { i, f ->
                    put(JSONObject().put("url", ShareStore.url(ClientServer.ORIGIN, token, i)).put("name", f.name)
                        .put("mime", f.mime).put("size", f.size))
                }
            })
            put("links", JSONArray(payload.links))
            put("text", payload.text)
        }
        if (payload.files.isNotEmpty()) state.nav.go(Dest.Web())
    }

    override fun closeShare() {
        state.share = null
    }

    override fun stayConnected(on: Boolean) {
        if (on && Build.VERSION.SDK_INT >= 33 && !has(Manifest.permission.POST_NOTIFICATIONS)) {
            askNotify.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        StayConnectedService.sync(this, on)
    }

    override fun openUrl(url: String) = openExternal(Uri.parse(url))

    // ── ClientWebView.Host ──────────────────────────────────────────────────────────────

    override fun askMedia(camera: Boolean, mic: Boolean, done: (Boolean, Boolean) -> Unit) {
        val need = buildList {
            if (camera && !has(Manifest.permission.CAMERA)) add(Manifest.permission.CAMERA)
            if (mic && !has(Manifest.permission.RECORD_AUDIO)) add(Manifest.permission.RECORD_AUDIO)
        }
        if (need.isEmpty()) return done(camera, mic)
        mediaWanted = camera to mic
        mediaDone = done
        askMedia.launch(need.toTypedArray())
    }

    override fun pickFiles(multiple: Boolean, mimeTypes: Array<String>, done: (Array<Uri>?) -> Unit) {
        filesDone = done
        val types = mimeTypes.ifEmpty { arrayOf("*/*") }
        if (multiple) pickMany.launch(types) else pickOne.launch(types)
    }

    override fun openExternal(uri: Uri) {
        runCatching { startActivity(Intent(Intent.ACTION_VIEW, uri).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }

    private var fullscreenExit: (() -> Unit)? = null

    override fun fullscreen(view: View?, exit: (() -> Unit)?) {
        state.fullscreen = view
        fullscreenExit = exit
        modes.active(view != null || state.active)
    }

    /** The page's `navigator.credentials` call, answered by the app's authenticator. */
    private fun passkey(o: JSONObject) {
        val id = o.optInt("id")
        val rpId = o.optString("rpId")
        val timeoutMs = o.optLong("timeoutMs", 60_000)
        val reply = { r: Authenticator.Result ->
            Hub.send(BridgeProtocol.WEBAUTHN_RESULT) {
                put("id", id)
                when (r) {
                    is Authenticator.Result.Made -> {
                        put("ok", true); put("rawId", Authenticator.enc(r.credId))
                        put("clientData", Authenticator.enc(r.clientData)); put("attestation", Authenticator.enc(r.attestation))
                    }
                    is Authenticator.Result.Signed -> {
                        put("ok", true); put("rawId", Authenticator.enc(r.credId))
                        put("clientData", Authenticator.enc(r.clientData)); put("authData", Authenticator.enc(r.authData))
                        put("signature", Authenticator.enc(r.signature))
                    }
                    is Authenticator.Result.Failed -> {
                        put("ok", false); put("errName", r.why.name); put("errCode", r.why.code); put("errMessage", r.why.message)
                    }
                }
            }
        }
        // Answered whatever happens: a request dropped here left the page waiting for ever.
        val challenge = runCatching { Authenticator.dec(o.optString("challenge")) }.getOrNull()
            ?: return reply(Authenticator.Result.Failed(io.github.sandptel.wado.passkey.AuthErrors.failed("the request was malformed")))
        val allow = runCatching {
            o.optJSONArray("allow")?.let { a -> (0 until a.length()).map { Authenticator.dec(a.getString(it)) } }.orEmpty()
        }.getOrElse { emptyList() }
        if (o.optString("op") == "create") authenticator.create(id, rpId, challenge, timeoutMs, reply)
        else authenticator.get(id, rpId, challenge, allow, timeoutMs, reply)
    }

    private fun has(p: String) = ContextCompat.checkSelfPermission(this, p) == PackageManager.PERMISSION_GRANTED
}
