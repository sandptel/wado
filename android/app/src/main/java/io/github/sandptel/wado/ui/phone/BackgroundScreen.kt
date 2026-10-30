package io.github.sandptel.wado.ui.phone

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.nav.PhonePage
import io.github.sandptel.wado.prefs.Prefs
import io.github.sandptel.wado.ui.common.Group
import io.github.sandptel.wado.ui.common.Toggle
import io.github.sandptel.wado.ui.common.TopBar
import io.github.sandptel.wado.ui.theme.LocalPalette

/** "Notifications & background": what only the phone app has — staying connected, the clipboard, volume keys. */
@Composable
fun BackgroundScreen(prefs: Prefs, onStayConnected: (Boolean) -> Unit, onBack: () -> Unit) {
    val p = LocalPalette.current
    LazyColumn(
        Modifier.fillMaxSize().background(p.ground).windowInsetsPadding(WindowInsets.safeDrawing),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        item { TopBar(PhonePage.Background.title, onBack) }
        item {
            Group {
                Toggle(
                    "Stay connected",
                    "Keeps wado's link to your computer open while the app is closed, so its notifications still arrive. Shows a quiet, permanent notification and uses some battery.",
                    prefs.stayConnected,
                ) { prefs.stayConnected = it; onStayConnected(it) }
                Toggle(
                    "Computer's copies to this phone",
                    "What the computer copies lands on this phone's clipboard.",
                    prefs.clipFromComputer,
                ) { prefs.clipFromComputer = it }
                Toggle(
                    "This phone's copies to the computer",
                    "When wado opens, what this phone copied goes to the computer. Never passwords: anything an app marks sensitive stays here.",
                    prefs.clipToComputer,
                ) { prefs.clipToComputer = it }
                Toggle(
                    "Volume keys control the computer",
                    "While wado is open, this phone's volume keys turn the computer's sound up and down.",
                    prefs.volumeKeys,
                ) { prefs.volumeKeys = it }
            }
        }
    }
}
