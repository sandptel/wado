package io.github.sandptel.wado.ui.settings

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.device.Haptics
import io.github.sandptel.wado.prefs.Prefs
import io.github.sandptel.wado.ui.common.LineIcon
import io.github.sandptel.wado.ui.common.Pressable
import io.github.sandptel.wado.ui.common.semanticsLabel
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Motion
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type

/**
 * The app's own switches — what only the Android app has. Everything about the computers and
 * the session lives in the client's settings ("Everything else" on home).
 */
@Composable
fun SettingsScreen(prefs: Prefs, onStayConnected: (Boolean) -> Unit, onAbout: () -> Unit, onBack: () -> Unit) {
    val p = LocalPalette.current
    LazyColumn(
        Modifier.fillMaxSize().background(p.ground).windowInsetsPadding(WindowInsets.safeDrawing),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        item { TopBar("This phone", onBack) }
        item {
            Group {
                Toggle(
                    "Stay connected",
                    "Keeps wado's link to your computer open while the app is closed, so its notifications still arrive. Shows a quiet, permanent notification and uses some battery. The web app installed from Chrome gets true push instead.",
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
        item {
            Pressable(onAbout, Modifier.fillMaxWidth().widthIn(max = 720.dp).heightIn(min = 60.dp), color = p.surface1, radius = Radius.r3) {
                Row(Modifier.padding(horizontal = 14.dp).heightIn(min = 60.dp), verticalAlignment = Alignment.CenterVertically) {
                    LineIcon("info", p.bright)
                    Spacer(Modifier.width(12.dp))
                    Text("About wado", style = Type.title, color = p.bright, modifier = Modifier.weight(1f))
                    LineIcon("right", p.dim, size = 18.dp)
                }
            }
        }
    }
}

@Composable
fun TopBar(title: String, onBack: () -> Unit) {
    val p = LocalPalette.current
    Row(Modifier.fillMaxWidth().widthIn(max = 720.dp).padding(bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Pressable(onBack, Modifier.size(44.dp).semanticsLabel("Back"), color = p.ground, radius = 22.dp, sink = 0.9f) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { LineIcon("back", p.bright) }
        }
        Spacer(Modifier.width(6.dp))
        Text(title, style = Type.heroName, color = p.bright)
    }
}

@Composable
private fun Group(content: @Composable () -> Unit) {
    val p = LocalPalette.current
    Column(
        Modifier.fillMaxWidth().widthIn(max = 720.dp).clip(RoundedCornerShape(Radius.r3)).background(p.surface1).padding(vertical = 4.dp),
    ) { content() }
}

/** A nav-row with DESIGN.md's switch (52 × 32, accent when on): the knob pops across. */
@Composable
private fun Toggle(title: String, sub: String, initial: Boolean, onChange: (Boolean) -> Unit) {
    val p = LocalPalette.current
    val view = LocalView.current
    var on by remember { mutableStateOf(initial) }
    val track by animateColorAsState(if (on) p.accent else p.surface3, Motion.spring(), label = "track")
    val knob by animateDpAsState(if (on) 23.dp else 3.dp, Motion.pop(), label = "knob")
    Pressable(
        {
            on = !on
            Haptics.perform(view, if (on) "toggle_on" else "toggle_off")
            onChange(on)
        },
        Modifier.fillMaxWidth(),
        color = p.surface1,
        radius = Radius.r2,
        sink = 0.99f,
    ) {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(title, style = Type.title, color = p.bright)
                Text(sub, style = Type.small, color = p.dim)
            }
            Spacer(Modifier.width(12.dp))
            Box(Modifier.size(width = 52.dp, height = 32.dp).clip(CircleShape).background(track)) {
                Box(Modifier.offset(x = knob, y = 3.dp).size(26.dp).clip(CircleShape).background(if (on) p.onAccent else p.bright))
            }
        }
    }
}
