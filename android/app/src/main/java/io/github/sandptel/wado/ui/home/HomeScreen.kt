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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.core.CardModel
import io.github.sandptel.wado.core.HomeModel
import io.github.sandptel.wado.core.RunningModel
import io.github.sandptel.wado.nav.PhonePage
import io.github.sandptel.wado.ui.common.LineIcon
import io.github.sandptel.wado.ui.common.Pressable
import io.github.sandptel.wado.ui.common.WadoButton
import io.github.sandptel.wado.ui.common.semanticsLabel
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Motion
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type

/** What home asks of the app. */
interface HomeActions {
    fun open(card: CardModel)
    fun play(card: CardModel)
    fun door(card: CardModel, go: String)
    fun pair()
    fun running(r: RunningModel)
    fun phone(page: PhonePage)
    fun web()
}

/**
 * The app's one native home (A7): the wordmark; Computers, each card from the core with its live
 * link; Running sessions, each naming its computer; On this phone; Add a computer. Drawn at once on
 * a cold start from the core's saved profiles; the link words arrive as the control rooms open.
 */
@Composable
fun HomeScreen(model: HomeModel, act: HomeActions) {
    val p = LocalPalette.current
    Box(Modifier.fillMaxSize().background(p.ground)) {
        LazyColumn(
            Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).testTag("home"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            item {
                Row(Modifier.fillMaxWidth().widthIn(max = 720.dp).padding(bottom = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                    Wordmark(Modifier.weight(1f))
                    IconAction("qr", "Pair a computer", act::pair)
                }
            }
            if (model.computers.isEmpty()) {
                item { Empty(act::pair) }
            } else {
                item { Heading("Computers") }
                itemsIndexed(model.computers, key = { _, c -> c.key }) { i, c ->
                    Arrive(i) {
                        ComputerCard(
                            c,
                            onOpen = { act.open(c) },
                            onPlay = { act.play(c) },
                            onDoor = { act.door(c, it) },
                            onPairApp = act::pair,
                            modifier = Modifier.widthIn(max = 720.dp),
                        )
                    }
                }
            }
            if (model.running.isNotEmpty()) {
                item { Heading("Running") }
                items(model.running, key = { "${it.computerKey}/${it.instance}" }) { r -> RunningRow(r) { act.running(r) } }
            }
            item { Heading("On this phone") }
            item { OnThisPhone(act::phone) }
            item {
                Row(Modifier.fillMaxWidth().widthIn(max = 720.dp).padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    WadoButton("Add a computer", act::pair, Modifier.weight(1f)) { LineIcon("plus", p.bright, size = 18.dp) }
                    WadoButton("More (web)", act::web, Modifier.weight(1f))
                }
            }
        }
    }
}

@Composable
private fun Heading(text: String) {
    val p = LocalPalette.current
    Text(
        text.uppercase(),
        style = Type.label,
        color = p.dim,
        modifier = Modifier.fillMaxWidth().widthIn(max = 720.dp).padding(start = 6.dp, top = 6.dp, bottom = 2.dp),
    )
}

/** A running session: what runs, and on which computer. */
@Composable
private fun RunningRow(r: RunningModel, onClick: () -> Unit) {
    val p = LocalPalette.current
    val what = r.appsFirst.ifEmpty { "An empty desktop" } + if (r.appsMore > 0) " +${r.appsMore}" else ""
    val who = buildList {
        add("on ${r.computerName}")
        if (r.lead.isNotEmpty()) add("sized for ${r.lead}")
        if (r.participants > 1) add("${r.participants} in it")
    }.joinToString(" · ")
    Pressable(onClick, Modifier.fillMaxWidth().widthIn(max = 720.dp).heightIn(min = 56.dp), color = p.surface1, radius = Radius.r3) {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            LineIcon("monitor", p.accent, size = 20.dp)
            Spacer(Modifier.size(12.dp))
            Column(Modifier.weight(1f)) {
                Text(what, style = Type.title, color = p.bright, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(who, style = Type.small, color = p.dim, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            LineIcon("right", p.dim, size = 18.dp)
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
    Pressable(onClick, Modifier.size(48.dp).semanticsLabel(label), color = p.ground, radius = 24.dp, sink = 0.9f) {
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
private fun Empty(onPair: () -> Unit) {
    val p = LocalPalette.current
    Column(
        Modifier.fillMaxWidth().widthIn(max = 520.dp).padding(top = 32.dp, bottom = 16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.size(84.dp).clip(CircleShape).background(p.surface1), contentAlignment = Alignment.Center) {
            LineIcon("monitor", p.accent, size = 36.dp)
        }
        Spacer(Modifier.height(4.dp))
        Text("Pair your first computer", style = Type.heroName, color = p.bright)
        Text(
            "On the computer, run wado pair (or wado tui → Pair) and scan the code it shows, or type its code.",
            style = Type.body,
            color = p.dim,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(8.dp))
        Pressable(onPair, Modifier.fillMaxWidth().height(58.dp), color = p.accent, radius = Radius.r3) {
            Row(Modifier.fillMaxSize(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) {
                LineIcon("qr", p.onAccent)
                Spacer(Modifier.size(10.dp))
                Text("Pair a computer", style = Type.headline, color = p.onAccent)
            }
        }
    }
}
