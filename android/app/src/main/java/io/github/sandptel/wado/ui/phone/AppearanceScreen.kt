package io.github.sandptel.wado.ui.phone

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.nav.PhonePage
import io.github.sandptel.wado.prefs.Prefs
import io.github.sandptel.wado.ui.common.Group
import io.github.sandptel.wado.ui.common.Pressable
import io.github.sandptel.wado.ui.common.Toggle
import io.github.sandptel.wado.ui.common.TopBar
import io.github.sandptel.wado.ui.theme.Base16
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Schemes
import io.github.sandptel.wado.ui.theme.Type

/**
 * Appearance (A7 "On this phone"): scheme, accent, corners, motion, and the Size pair — Text size
 * (85–150 % or follow the phone) and Interface size — with a live sample. The slider applies on
 * release, so a page never reflows under the finger.
 */
@Composable
fun AppearanceScreen(prefs: Prefs, onBack: () -> Unit) {
    val p = LocalPalette.current
    val context = LocalContext.current
    val look by LookStore.look.collectAsState()
    val set = { l: Look -> LookStore.set(prefs, l) }
    var dragging by remember { mutableFloatStateOf(-1f) }
    val shownText = if (dragging > 0f) dragging else look.size.text
    LazyColumn(
        Modifier.fillMaxSize().background(p.ground).windowInsetsPadding(WindowInsets.safeDrawing),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        item { TopBar(PhonePage.Appearance.title, onBack) }
        item {
            // The sample: drawn at the size being chosen, before it applies.
            SizeScope(look.size.copy(text = shownText, follow = look.size.follow && dragging < 0f)) {
                Column(
                    Modifier.fillMaxWidth().widthIn(max = 720.dp).clip(RoundedCornerShape(Radius.r3)).background(p.surface1).padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Text("linux-box", style = Type.headline, color = p.bright)
                    Text("NixOS · Hyprland", style = Type.mono, color = p.dim)
                    Text("This is how pages read at this size.", style = Type.body, color = p.text)
                }
            }
        }
        item { Section("Scheme") }
        item {
            Chips(
                listOf("" to "Follow the web part") + Schemes.list(context).map { it.first to it.first },
                look.scheme,
            ) { set(look.copy(scheme = it)) }
        }
        item { Section("Accent") }
        item {
            val base = Schemes.byName(context, look.scheme) ?: Base16.DEFAULT
            Row(Modifier.fillMaxWidth().widthIn(max = 720.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                (-1..7).forEach { slot ->
                    val c = Color(if (slot < 0) base.base(0xD) else base.base(0x8 + slot))
                    val on = look.accent == slot
                    Box(
                        Modifier.size(48.dp).clip(CircleShape).background(p.surface1)
                            .border(if (on) 3.dp else 0.dp, p.bright, CircleShape)
                            .semantics { contentDescription = if (slot < 0) "Scheme's own accent" else "Accent ${slot + 1}"; selected = on },
                        contentAlignment = Alignment.Center,
                    ) {
                        Pressable({ set(look.copy(accent = slot)) }, Modifier.size(34.dp), color = c, radius = 17.dp) {}
                    }
                }
            }
        }
        item { Section("Corners") }
        item { Chips(listOf("soft" to "Soft", "round" to "Round", "square" to "Square"), look.corners) { set(look.copy(corners = it)) } }
        item { Section("Motion") }
        item { Chips(listOf("full" to "Full", "reduced" to "Reduced", "off" to "Off"), look.motion) { set(look.copy(motion = it)) } }
        item { Section("Text size") }
        item {
            Group {
                Toggle("Follow phone", "Use this phone's own font size", look.size.follow) {
                    set(look.copy(size = look.size.copy(follow = it)))
                }
                if (!look.size.follow) {
                    Column(Modifier.padding(horizontal = 14.dp, vertical = 8.dp)) {
                        Text("${(shownText * 100).toInt()} %", style = Type.numeral, color = p.bright)
                        Slider(
                            value = shownText,
                            onValueChange = { dragging = SizeModel.snapText(it) },
                            onValueChangeFinished = {
                                if (dragging > 0f) set(look.copy(size = look.size.copy(text = dragging)))
                                dragging = -1f
                            },
                            valueRange = 0.85f..1.5f,
                            steps = SizeModel.TEXT_STEPS.size - 2,
                            colors = SliderDefaults.colors(thumbColor = p.accent, activeTrackColor = p.accent, inactiveTrackColor = p.surface3),
                            modifier = Modifier.heightIn(min = 48.dp),
                        )
                    }
                }
            }
        }
        item { Section("Interface size") }
        item {
            Chips(SizeModel.UI_STEPS.map { it.second.toString() to it.first }, look.size.ui.toString()) { v ->
                set(look.copy(size = look.size.copy(ui = v.toFloat())))
            }
        }
    }
}

@Composable
private fun Section(text: String) {
    val p = LocalPalette.current
    Text(text, style = Type.title, color = p.bright, modifier = Modifier.fillMaxWidth().widthIn(max = 720.dp).padding(start = 4.dp, top = 6.dp))
}

/** A row of choices, the chosen one filled with the accent. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Chips(options: List<Pair<String, String>>, chosen: String, onPick: (String) -> Unit) {
    val p = LocalPalette.current
    FlowRow(
        Modifier.fillMaxWidth().widthIn(max = 720.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        options.forEach { (value, label) ->
            val on = value == chosen
            Pressable(
                { onPick(value) },
                Modifier.heightIn(min = 48.dp).semantics { selected = on },
                color = if (on) p.accent else p.surface2,
                radius = Radius.r2,
            ) {
                Box(Modifier.heightIn(min = 48.dp).padding(horizontal = 14.dp), contentAlignment = Alignment.Center) {
                    Text(label, style = Type.title, color = if (on) p.onAccent else p.bright)
                }
            }
        }
    }
}
