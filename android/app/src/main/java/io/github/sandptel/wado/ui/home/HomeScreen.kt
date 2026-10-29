package io.github.sandptel.wado.ui.home

import androidx.compose.animation.core.Animatable
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
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.bridge.Computer
import io.github.sandptel.wado.bridge.Computers
import io.github.sandptel.wado.online.OnlineProbe
import io.github.sandptel.wado.ui.common.LineIcon
import io.github.sandptel.wado.ui.common.Pressable
import io.github.sandptel.wado.ui.common.WadoButton
import io.github.sandptel.wado.ui.common.semanticsLabel
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Motion
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type
import kotlinx.coroutines.delay

/**
 * The app's own home: the computers this phone has, each with its live status and its doors.
 * Drawn natively from the copy the page last sent, so it is up at once on a cold start; every
 * door opens the client at that computer.
 */
@Composable
fun HomeScreen(
    computers: Computers,
    onOpen: (Computer) -> Unit,
    onDoor: (Computer, String) -> Unit,
    onPair: () -> Unit,
    onTypeCode: () -> Unit,
    onSettings: () -> Unit,
    onAll: () -> Unit,
) {
    val p = LocalPalette.current
    var seen by remember { mutableStateOf<Map<String, OnlineProbe.Seen>>(emptyMap()) }
    LaunchedEffect(computers.list) {
        while (true) {
            seen = OnlineProbe.probeAll(computers.list)
            delay(20_000)
        }
    }
    Box(Modifier.fillMaxSize().background(p.ground)) {
        LazyColumn(
            Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            item {
                Row(Modifier.fillMaxWidth().widthIn(max = 720.dp).padding(bottom = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                    Wordmark(Modifier.weight(1f))
                    IconAction("qr", "Pair a computer", onPair)
                    IconAction("gear", "Settings and about", onSettings)
                }
            }
            if (computers.list.isEmpty()) {
                item { Empty(onPair, onTypeCode) }
            } else {
                item {
                    Text(
                        "COMPUTERS",
                        style = Type.label,
                        color = p.dim,
                        modifier = Modifier.fillMaxWidth().widthIn(max = 720.dp).padding(start = 6.dp, bottom = 2.dp),
                    )
                }
                itemsIndexed(computers.list, key = { _, c -> c.key }) { i, c ->
                    Arrive(i) {
                        ComputerCard(
                            c,
                            seen[c.key] ?: OnlineProbe.Seen.UNKNOWN,
                            selected = i == computers.selected && computers.list.size > 1,
                            onOpen = { onOpen(c) },
                            onDoor = { onDoor(c, it) },
                            modifier = Modifier.widthIn(max = 720.dp),
                        )
                    }
                }
                item {
                    Row(Modifier.fillMaxWidth().widthIn(max = 720.dp).padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        WadoButton("Add a computer", onPair, Modifier.weight(1f)) { LineIcon("plus", p.bright, size = 18.dp) }
                        WadoButton("Everything else", onAll, Modifier.weight(1f))
                    }
                }
            }
        }
    }
}

/** "wado" with its accent dot — the same mark as the launcher icon. */
@Composable
fun Wordmark(modifier: Modifier = Modifier) {
    val p = LocalPalette.current
    Text(
        buildAnnotatedString {
            append("wado")
            withStyle(SpanStyle(color = p.accent)) { append(".") }
        },
        style = Type.display.copy(fontSize = Type.display.fontSize * 0.75f),
        color = p.bright,
        modifier = modifier,
    )
}

@Composable
private fun IconAction(icon: String, label: String, onClick: () -> Unit) {
    val p = LocalPalette.current
    Pressable(onClick, Modifier.size(44.dp).semanticsLabel(label), color = p.ground, radius = 22.dp, sink = 0.9f) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { LineIcon(icon, p.dim) }
    }
}

/** Cards rise into place one after another (a micro cascade, well under half a second). */
@Composable
private fun Arrive(index: Int, content: @Composable () -> Unit) {
    val t = remember { Animatable(if (Motion.reduced) 1f else 0f) }
    LaunchedEffect(Unit) { t.animateTo(1f, Motion.spring(Motion.DUR_SHEET, delay = (index * Motion.STAGGER).coerceAtMost(200))) }
    Box(Modifier.graphicsLayer { alpha = t.value; translationY = (1f - t.value) * 24.dp.toPx() }) { content() }
}

@Composable
private fun Empty(onPair: () -> Unit, onTypeCode: () -> Unit) {
    val p = LocalPalette.current
    Column(
        Modifier.fillMaxWidth().widthIn(max = 520.dp).padding(top = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.size(84.dp).clip(CircleShape).background(p.surface1), contentAlignment = Alignment.Center) {
            LineIcon("monitor", p.accent, size = 36.dp)
        }
        Spacer(Modifier.height(4.dp))
        Text("Pair your first computer", style = Type.heroName, color = p.bright)
        Text(
            "On the computer, run wado tui (or wado qr) and scan the code it shows. That's the whole setup.",
            style = Type.body,
            color = p.dim,
            textAlign = androidx.compose.ui.text.style.TextAlign.Center,
        )
        Spacer(Modifier.height(8.dp))
        Pressable(onPair, Modifier.fillMaxWidth().height(58.dp), color = p.accent, radius = Radius.r3) {
            Row(Modifier.fillMaxSize(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) {
                LineIcon("qr", p.onAccent)
                Spacer(Modifier.size(10.dp))
                Text("Scan its code", style = Type.headline, color = p.onAccent)
            }
        }
        WadoButton("Type a code instead", onTypeCode, Modifier.fillMaxWidth())
    }
}
