package io.github.sandptel.wado

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.view.KeyEvent
import android.view.View
import androidx.activity.ComponentActivity
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
import io.github.sandptel.wado.share.ShareIntent
import io.github.sandptel.wado.share.ShareStore
import io.github.sandptel.wado.shortcuts.Shortcuts
import io.github.sandptel.wado.stay.StayConnectedService
import io.github.sandptel.wado.ui.AppActions
import io.github.sandptel.wado.ui.AppRoot
import io.github.sandptel.wado.ui.AppState
import io.github.sandptel.wado.ui.Route
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
class MainActivity : ComponentActivity(), ClientWebView.Host, AppActions {
    private val app get() = application as WadoApp
    private val state = AppState()
    private lateinit var web: ClientWebView
    private lateinit var modes: ScreenModes
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
        Hub.screen = object : Hub.Handlers {
            override fun haptic(kind: String) = Haptics.perform(window.decorView, kind)
            override fun active(on: Boolean) {
                state.active = on
                modes.active(on)
            }
            override fun orient(lock: String) = modes.orient(lock)
        }
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
                AppRoot(web.view, state, computers, app.prefs, this)
            }
        }
        if (fresh) route(intent, firstLoad = true) else route(intent, firstLoad = false)
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
            if (door.door == "trackpad" && c != null) {
                state.pad = c
                state.route = Route.Pad
            } else door(c, door.door)
            return
        }
        DeepLinks.connect(data)?.let { link -> if (firstLoad) state.route = Route.Web else connect(link) }
    }

    private fun event(t: String, o: JSONObject) {
        when (t) {
            BridgeProtocol.EXIT -> state.route = Route.Home
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
            state.route == Route.Web -> if (web.view.canGoBack()) web.view.goBack() else Hub.send(BridgeProtocol.BACK)
            else -> state.route.back()?.let { state.route = it } ?: moveTaskToBack(true)
        }
    }

    // ── AppActions ──────────────────────────────────────────────────────────────────────

    override fun open(c: Computer) {
        Hub.send(BridgeProtocol.GO) { put("key", c.key); put("go", "") }
        state.route = Route.Web
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
        state.route = Route.Web
    }

    override fun connect(link: Map<String, String>) {
        web.loadHome(DeepLinks.connectQuery(link))
        state.route = Route.Web
    }

    override fun showWeb() {
        state.route = Route.Web
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
        if (payload.files.isNotEmpty()) state.route = Route.Web
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

    private fun has(p: String) = ContextCompat.checkSelfPermission(this, p) == PackageManager.PERMISSION_GRANTED
}
