package io.github.sandptel.wado.ui

import android.widget.Toast
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LifecycleStartEffect
import io.github.sandptel.wado.bridge.Computers
import io.github.sandptel.wado.core.CardModel
import io.github.sandptel.wado.core.CoreHost
import io.github.sandptel.wado.core.RunningModel
import io.github.sandptel.wado.nav.Dest
import io.github.sandptel.wado.nav.Navigator
import io.github.sandptel.wado.nav.PhonePage
import io.github.sandptel.wado.prefs.Prefs
import io.github.sandptel.wado.ui.about.AboutScreen
import io.github.sandptel.wado.ui.home.HomeActions
import io.github.sandptel.wado.ui.home.HomeScreen
import io.github.sandptel.wado.ui.pad.PadScreen
import io.github.sandptel.wado.ui.pair.PairScreen
import io.github.sandptel.wado.ui.pair.PairTypedScreen
import io.github.sandptel.wado.ui.phone.AppearanceScreen
import io.github.sandptel.wado.ui.phone.BackgroundScreen
import io.github.sandptel.wado.ui.phone.DiagnosticsScreen
import io.github.sandptel.wado.ui.phone.LookStore
import io.github.sandptel.wado.ui.phone.SizeScope

/** Which screen draws each page: a thin dispatcher, no logic of its own. */
@Composable
fun Pages(dest: Dest, nav: Navigator, computers: Computers, core: CoreHost, prefs: Prefs, act: AppActions) {
    val look by LookStore.look.collectAsState()
    when (dest) {
        // The web layer is underneath: this page is see-through.
        is Dest.Web -> Box(Modifier.fillMaxSize())
        // The pad is a surface, not a page: the Size model never touches it.
        is Dest.Pad -> computers.list.firstOrNull { it.key == dest.key }?.let { c ->
            PadScreen(c, act.padModes, onMore = { act.door(c, "trackpad") }) { nav.pop() }
        } ?: LaunchedEffect(Unit) { nav.pop() }
        else -> SizeScope(look.size) { Page(dest, nav, computers, core, prefs, act) }
    }
}

@Composable
private fun Page(dest: Dest, nav: Navigator, computers: Computers, core: CoreHost, prefs: Prefs, act: AppActions) {
    when (dest) {
        is Dest.Web, is Dest.Pad -> {}
        Dest.Home -> Home(nav, computers, core, act)
        Dest.Pair -> PairScreen(core, onTyped = { nav.go(Dest.PairTyped) }, onDone = { nav.home() }) { nav.pop() }
        Dest.PairTyped -> PairTypedScreen(core, onDone = { nav.home() }) { nav.pop() }
        is Dest.Phone -> when (dest.page) {
            PhonePage.Appearance -> AppearanceScreen(prefs) { nav.pop() }
            PhonePage.Background -> BackgroundScreen(prefs, act::stayConnected) { nav.pop() }
            PhonePage.Diagnostics -> DiagnosticsScreen(core) { nav.pop() }
            PhonePage.About -> AboutScreen(act::openUrl) { nav.pop() }
        }
    }
}

@Composable
private fun Home(nav: Navigator, computers: Computers, core: CoreHost, act: AppActions) {
    val model by core.model.collectAsState()
    val ready by core.core.collectAsState()
    val context = LocalContext.current
    // Hold a control room for every computer on screen, only while home is in front.
    val keys = model.computers.filter { !it.needsAppPairing }.map { it.key }
    LifecycleStartEffect(keys, ready) {
        core.watch(keys)
        onStopOrDispose { core.watch(emptyList()) }
    }
    val web = { c: CardModel -> computers.list.firstOrNull { it.key == c.webKey } }
    HomeScreen(model, object : HomeActions {
        fun webDoor(c: CardModel, go: String) {
            val w = web(c)
            if (w != null) {
                act.door(w, go)
            } else {
                // The app's core paired it; the web part has not met it yet (A9.5).
                Toast.makeText(context, "Open this once in the web part to pair it there too", Toast.LENGTH_LONG).show()
                nav.go(Dest.Web())
            }
        }

        override fun open(card: CardModel) = webDoor(card, "")
        override fun play(card: CardModel) = webDoor(card, "session")
        override fun door(card: CardModel, go: String) {
            val w = web(card)
            if (go == "trackpad" && w != null) nav.go(Dest.Pad(w.key)) else webDoor(card, go)
        }
        override fun pair() = nav.go(Dest.Pair)
        override fun running(r: RunningModel) {
            model.computers.firstOrNull { it.key == r.computerKey }?.let { webDoor(it, "") }
        }
        override fun phone(page: PhonePage) = nav.go(Dest.Phone(page))
        override fun web() = nav.go(Dest.Web())
    })
}
