package io.github.sandptel.wado.ui.home

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.core.CardModel
import io.github.sandptel.wado.ui.common.LineIcon
import io.github.sandptel.wado.ui.common.Pressable
import io.github.sandptel.wado.ui.common.WadoButton
import io.github.sandptel.wado.ui.common.semanticsLabel
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type

/** The doors on a computer that open the web part (or the native trackpad). */
val DOORS = listOf(
    Triple("trackpad", "Trackpad", "trackpad"),
    Triple("files", "Files", "folder"),
    Triple("shell", "Shell", "term"),
)

/**
 * One computer (A7's card): its OS logo in a disc of its hue, its name, OS · compositor, the link's
 * honest words and its sessions, and a ▶ for its desktop — accent, or solid with a glow while a
 * session runs; disabled, saying why, while it is offline. A computer only the web part knows
 * offers "Pair the app" instead (the app's core is a new device, A9.5).
 */
@Composable
fun ComputerCard(
    card: CardModel,
    onOpen: () -> Unit,
    onPlay: () -> Unit,
    onDoor: (String) -> Unit,
    onPairApp: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val p = LocalPalette.current
    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Radius.r3))
            .background(p.surface1)
            .border(1.dp, p.line, RoundedCornerShape(Radius.r3))
            .padding(6.dp),
    ) {
        Row(Modifier.fillMaxWidth().heightIn(min = 80.dp), verticalAlignment = Alignment.CenterVertically) {
            Pressable(onOpen, Modifier.weight(1f).heightIn(min = 72.dp), color = p.surface1, radius = Radius.r2, sink = 0.985f) {
                Row(Modifier.padding(horizontal = 8.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                    OsLogoDisc(card.logo, card.hue)
                    Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(card.name, style = Type.headline, color = p.bright, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        if (card.osLine.isNotEmpty()) {
                            Text(card.osLine, style = Type.mono, color = p.dim, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                        if (card.needsAppPairing) {
                            Text("Known to the web part only", style = Type.small, color = p.dim)
                        } else {
                            val n = card.sessions
                            LinkWords(card.link, if (n > 0) " · $n session${if (n == 1) "" else "s"}" else "")
                        }
                    }
                }
            }
            if (!card.needsAppPairing) PlayButton(card, onPlay)
        }
        if (card.needsAppPairing) {
            WadoButton("Pair the app", onPairApp, Modifier.fillMaxWidth().padding(4.dp), primary = true) {
                LineIcon("qr", p.onAccent, size = 18.dp)
            }
        } else if (card.webKey != null) {
            Spacer(Modifier.height(2.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                DOORS.forEach { (go, label, icon) ->
                    Pressable({ onDoor(go) }, Modifier.weight(1f).heightIn(min = 48.dp).semanticsLabel("$label on ${card.name}"), radius = Radius.r2) {
                        Row(
                            Modifier.fillMaxWidth().heightIn(min = 48.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
                        ) {
                            LineIcon(icon, p.bright, size = 18.dp)
                            Text(label, style = Type.small, color = p.text, maxLines = 1)
                        }
                    }
                }
            }
        }
    }
}

/** The 48 dp ▶ for the desktop. */
@Composable
private fun PlayButton(card: CardModel, onPlay: () -> Unit) {
    val p = LocalPalette.current
    val running = card.sessions > 0
    if (!card.link.online) {
        Box(
            Modifier.size(48.dp).clip(CircleShape).background(p.surface2)
                .semantics { contentDescription = "Desktop — ${card.name} is offline"; disabled() },
            contentAlignment = Alignment.Center,
        ) { LineIcon("play", p.dim, size = 20.dp) }
        return
    }
    val glow = if (running) Modifier.shadow(12.dp, CircleShape, ambientColor = p.accent, spotColor = p.accent) else Modifier
    Pressable(
        onPlay,
        glow.size(48.dp).semanticsLabel(if (running) "Desktop on ${card.name} — a session is running" else "Desktop on ${card.name}"),
        color = if (running) p.accent else p.surface2,
        radius = 24.dp,
        sink = 0.92f,
    ) {
        Box(Modifier.size(48.dp), contentAlignment = Alignment.Center) {
            LineIcon("play", if (running) p.onAccent else p.accent, size = 20.dp)
        }
    }
}
