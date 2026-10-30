package io.github.sandptel.wado.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.bridge.Computers
import io.github.sandptel.wado.nav.Dest
import io.github.sandptel.wado.nav.Navigator
import io.github.sandptel.wado.nav.PhonePage
import io.github.sandptel.wado.prefs.Prefs
import io.github.sandptel.wado.ui.about.AboutScreen
import io.github.sandptel.wado.ui.home.HomeScreen
import io.github.sandptel.wado.ui.pad.PadScreen
import io.github.sandptel.wado.ui.pair.PairScreen
import io.github.sandptel.wado.ui.settings.SettingsScreen
import io.github.sandptel.wado.ui.settings.TopBar
import io.github.sandptel.wado.ui.theme.LocalPalette

/** Which screen draws each page: a thin dispatcher, no logic of its own. */
@Composable
fun Pages(dest: Dest, nav: Navigator, computers: Computers, prefs: Prefs, act: AppActions) {
    when (dest) {
        // The web layer is underneath: this page is see-through.
        is Dest.Web -> Box(Modifier.fillMaxSize())
        Dest.Home -> HomeScreen(
            computers,
            onOpen = { c -> act.door(c, "") },
            onDoor = { c, go -> if (go == "trackpad") nav.go(Dest.Pad(c.key)) else act.door(c, go) },
            onPair = { nav.go(Dest.Pair) },
            onTypeCode = { nav.go(Dest.Web()) },
            onSettings = { nav.go(Dest.Phone(PhonePage.Background)) },
            onAll = { nav.go(Dest.Web()) },
        )
        Dest.Pair -> PairScreen(act::connect, { nav.go(Dest.Web()) }) { nav.pop() }
        Dest.PairTyped -> LaunchedEffect(Unit) { nav.pop() }
        is Dest.Phone -> when (dest.page) {
            PhonePage.Background -> SettingsScreen(prefs, act::stayConnected, { nav.go(Dest.Phone(PhonePage.About)) }) { nav.pop() }
            PhonePage.About -> AboutScreen(act::openUrl) { nav.pop() }
            else -> PhoneStub(dest.page.title) { nav.pop() }
        }
        is Dest.Pad -> computers.list.firstOrNull { it.key == dest.key }?.let { c ->
            PadScreen(c, act.padModes, onMore = { act.door(c, "trackpad") }) { nav.pop() }
        } ?: LaunchedEffect(Unit) { nav.pop() }
    }
}

/** A page of "On this phone" not built yet: its title, and Back. */
@Composable
private fun PhoneStub(title: String, onBack: () -> Unit) {
    val p = LocalPalette.current
    Box(Modifier.fillMaxSize().background(p.ground).windowInsetsPadding(WindowInsets.safeDrawing).padding(16.dp)) {
        TopBar(title, onBack)
    }
}
