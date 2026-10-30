package io.github.sandptel.wado.ui

import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateFloatAsState
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
import io.github.sandptel.wado.nav.AppNav
import io.github.sandptel.wado.nav.Dest
import io.github.sandptel.wado.nav.Navigator
import io.github.sandptel.wado.share.SharePayload
import io.github.sandptel.wado.ui.pad.PadModes
import io.github.sandptel.wado.ui.share.SharePicker
import io.github.sandptel.wado.ui.share.ShareState
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Motion
import io.github.sandptel.wado.ui.theme.Type

/** The screen state the activity drives. */
class AppState {
    /** The pages, on one saved back stack. */
    val nav = Navigator()
    var share by mutableStateOf<SharePayload?>(null)
    var shareState by mutableStateOf<ShareState>(ShareState.Choosing)
    /** A page element in fullscreen (a video): drawn over everything. */
    var fullscreen by mutableStateOf<View?>(null)
    /** The page asked for the screen to itself (a session, the trackpad): no inset padding. */
    var active by mutableStateOf(false)
    /** The computer's volume, shown briefly after a volume key; null when hidden. */
    var volumeHud by mutableStateOf<Float?>(null)
}

/** What the screens ask the activity to do. */
interface AppActions {
    fun door(c: Computer?, go: String)
    val padModes: PadModes
    fun pick(c: Computer)
    /** A scanned connect link, for the web part. */
    fun connect(link: Map<String, String>)
    fun closeShare()
    fun exitFullscreen()
    /** Back on the web layer: the page's own history first, then the page decides. */
    fun webBack()
    fun stayConnected(on: Boolean)
    fun openUrl(url: String)
}

/**
 * The client's WebView at the bottom, always there (it holds the link); the native pages over it
 * on one saved back stack ([Pages]); the share sheet and a fullscreen element over those.
 */
@Composable
fun AppRoot(web: WebView, state: AppState, computers: Computers, pages: @Composable (Dest) -> Unit, act: AppActions) {
    val p = LocalPalette.current
    val nav = state.nav
    val onWeb = nav.top is Dest.Web
    Box(Modifier.fillMaxSize().background(if (onWeb) Color.Black else p.ground)) {
        val webPad = if (state.active) Modifier else Modifier.background(p.ground).windowInsetsPadding(WindowInsets.safeDrawing)
        AndroidView(
            factory = { (web.parent as? ViewGroup)?.removeView(web); web },
            modifier = Modifier.fillMaxSize().then(webPad),
        )
        AppNav(nav, onBack = { top -> if (top is Dest.Web) act.webBack() else nav.pop() }, content = pages)
        state.share?.let { payload ->
            SharePicker(payload, computers, state.shareState, act::pick, act::closeShare)
        }
        BackHandler(state.share != null) { act.closeShare() }
        state.fullscreen?.let { v ->
            AndroidView(factory = { (v.parent as? ViewGroup)?.removeView(v); v }, modifier = Modifier.fillMaxSize().background(Color.Black))
        }
        BackHandler(state.fullscreen != null) { act.exitFullscreen() }
        VolumeHud(state.volumeHud)
    }
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
