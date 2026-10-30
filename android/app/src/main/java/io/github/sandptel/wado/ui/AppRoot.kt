package io.github.sandptel.wado.ui

import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import io.github.sandptel.wado.bridge.Computer
import io.github.sandptel.wado.bridge.Computers
import io.github.sandptel.wado.prefs.Prefs
import io.github.sandptel.wado.share.SharePayload
import io.github.sandptel.wado.ui.about.AboutScreen
import io.github.sandptel.wado.ui.home.HomeScreen
import io.github.sandptel.wado.ui.pad.PadModes
import io.github.sandptel.wado.ui.pad.PadScreen
import io.github.sandptel.wado.ui.pair.PairScreen
import io.github.sandptel.wado.ui.settings.SettingsScreen
import io.github.sandptel.wado.ui.share.SharePicker
import io.github.sandptel.wado.ui.share.ShareState
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Motion
import io.github.sandptel.wado.ui.theme.Type

/** The screen state the activity drives. */
class AppState {
    var route by mutableStateOf(Route.Home)
    var share by mutableStateOf<SharePayload?>(null)
    var shareState by mutableStateOf<ShareState>(ShareState.Choosing)
    /** A page element in fullscreen (a video): drawn over everything. */
    var fullscreen by mutableStateOf<View?>(null)
    /** The computer the native trackpad drives. */
    var pad by mutableStateOf<Computer?>(null)
    /** The page asked for the screen to itself (a session, the trackpad): no inset padding. */
    var active by mutableStateOf(false)
    /** The computer's volume, shown briefly after a volume key; null when hidden. */
    var volumeHud by mutableStateOf<Float?>(null)
}

/** What the screens ask the activity to do. */
interface AppActions {
    fun open(c: Computer)
    fun door(c: Computer?, go: String)
    val padModes: PadModes
    fun connect(link: Map<String, String>)
    fun showWeb()
    fun pick(c: Computer)
    fun closeShare()
    fun stayConnected(on: Boolean)
    fun openUrl(url: String)
}

/**
 * The client's WebView at the bottom, always there (it holds the link); the native screens over
 * it, moving on the page curve; the share sheet and a fullscreen element over those.
 */
@Composable
fun AppRoot(web: WebView, state: AppState, computers: Computers, prefs: Prefs, act: AppActions) {
    val p = LocalPalette.current
    Box(Modifier.fillMaxSize().background(if (state.route == Route.Web) Color.Black else p.ground)) {
        val webPad = if (state.active) Modifier else Modifier.background(p.ground).windowInsetsPadding(WindowInsets.safeDrawing)
        AndroidView(
            factory = { (web.parent as? ViewGroup)?.removeView(web); web },
            modifier = Modifier.fillMaxSize().then(webPad),
        )
        AnimatedContent(
            targetState = state.route,
            transitionSpec = { pageTransition(initialState, targetState) },
            label = "route",
        ) { route ->
            when (route) {
                Route.Web -> Box(Modifier.fillMaxSize())
                Route.Home -> HomeScreen(
                    computers,
                    onOpen = act::open,
                    onDoor = { c, go ->
                        // The trackpad is native; the other doors open the client.
                        if (go == "trackpad") {
                            state.pad = c
                            state.route = Route.Pad
                        } else act.door(c, go)
                    },
                    onPair = { state.route = Route.Pair },
                    onTypeCode = act::showWeb,
                    onSettings = { state.route = Route.Settings },
                    onAll = act::showWeb,
                )
                Route.Pair -> PairScreen(act::connect, act::showWeb) { state.route = Route.Home }
                Route.Pad -> state.pad?.let { c ->
                    PadScreen(c, act.padModes, onMore = { act.door(c, "trackpad") }) { state.route = Route.Home }
                } ?: LaunchedEffect(Unit) { state.route = Route.Home }
                Route.Settings -> SettingsScreen(prefs, act::stayConnected, { state.route = Route.About }) { state.route = Route.Home }
                Route.About -> AboutScreen(act::openUrl) { state.route = Route.Settings }
            }
        }
        state.share?.let { payload ->
            SharePicker(payload, computers, state.shareState, act::pick, act::closeShare)
        }
        state.fullscreen?.let { v ->
            AndroidView(factory = { (v.parent as? ViewGroup)?.removeView(v); v }, modifier = Modifier.fillMaxSize().background(Color.Black))
        }
        VolumeHud(state.volumeHud)
    }
}

/** Forward pages slide in from the right on the spring; back slides out the other way, quicker. */
private fun pageTransition(from: Route, to: Route): ContentTransform {
    if (Motion.reduced) return fadeIn(Motion.spring(0)) togetherWith fadeOut(Motion.spring(0))
    // The web layer is already there underneath: native screens fade off it and back onto it.
    if (from == Route.Web || to == Route.Web) {
        return fadeIn(Motion.spring(Motion.DUR)) togetherWith fadeOut(Motion.exit())
    }
    val forward = to.depth > from.depth
    val enter = slideInHorizontally(Motion.spring(Motion.DUR_SHEET)) { w -> if (forward) w / 3 else -w / 3 } +
        fadeIn(Motion.spring(Motion.DUR))
    val exit = slideOutHorizontally(Motion.spring(Motion.DUR_SHEET_EXIT)) { w -> if (forward) -w / 4 else w / 3 } +
        fadeOut(Motion.exit())
    return enter togetherWith exit
}

@Composable
private fun VolumeHud(level: Float?) {
    val p = LocalPalette.current
    val shown by animateFloatAsState(if (level != null) 1f else 0f, Motion.pop(), label = "hud")
    if (shown <= 0.01f) return
    Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing), contentAlignment = Alignment.TopCenter) {
        Box(
            Modifier.padding(top = 12.dp)
                .graphicsLayer { alpha = shown; scaleX = 0.9f + 0.1f * shown; scaleY = scaleX }
                .clip(CircleShape).background(p.surface2).padding(horizontal = 16.dp, vertical = 8.dp),
        ) {
            Text("Computer volume  ${((level ?: 0f) * 100).toInt()}%", style = Type.numeral, color = p.bright)
        }
    }
}
