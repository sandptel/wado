package io.github.sandptel.wado.ui.home

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.bridge.Computer
import io.github.sandptel.wado.online.OnlineProbe.Seen
import io.github.sandptel.wado.ui.common.LineIcon
import io.github.sandptel.wado.ui.common.Pressable
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Motion
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type

/** The four doors on a computer — the same as the app-icon shortcuts. */
val DOORS = listOf(
    Triple("session", "Desktop", "monitor"),
    Triple("trackpad", "Trackpad", "trackpad"),
    Triple("files", "Files", "folder"),
    Triple("shell", "Shell", "term"),
)

/**
 * One computer: its name and honest status (DESIGN.md: the true state, never a softened
 * "connected"), the card itself opening its page, and its four doors as tiles below.
 */
@Composable
fun ComputerCard(
    computer: Computer,
    seen: Seen,
    selected: Boolean,
    onOpen: () -> Unit,
    onDoor: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val p = LocalPalette.current
    Column(
        modifier
            .fillMaxWidth()
            .clip(androidx.compose.foundation.shape.RoundedCornerShape(Radius.r3))
            .background(p.surface1)
            .padding(8.dp),
    ) {
        Pressable(onOpen, Modifier.fillMaxWidth(), color = p.surface1, radius = Radius.r2, sink = 0.985f) {
            Row(Modifier.padding(horizontal = 10.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(computer.title, style = Type.headline, color = p.bright, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Spacer(Modifier.height(4.dp))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        StatusDot(seen)
                        Spacer(Modifier.width(6.dp))
                        Text(
                            status(seen) + if (selected) " · last used" else "",
                            style = Type.small,
                            color = p.dim,
                        )
                    }
                }
                LineIcon("right", p.dim, size = 20.dp)
            }
        }
        Spacer(Modifier.height(6.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            DOORS.forEach { (go, label, icon) ->
                Pressable({ onDoor(go) }, Modifier.weight(1f).height(62.dp), radius = Radius.r2) {
                    Column(
                        Modifier.fillMaxWidth().padding(vertical = 9.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        LineIcon(icon, p.bright, size = 20.dp)
                        Text(label, style = Type.small, color = p.text, maxLines = 1)
                    }
                }
            }
        }
    }
}

fun status(seen: Seen) = when (seen) {
    Seen.ONLINE -> "Online"
    Seen.OFFLINE -> "Offline"
    Seen.UNREACHABLE -> "Can't reach its relay"
    Seen.UNKNOWN -> "Checking…"
}

/** Online breathes (the only ambient motion here); offline is still and dim. */
@Composable
fun StatusDot(seen: Seen) {
    val p = LocalPalette.current
    val color = when (seen) {
        Seen.ONLINE -> p.good
        Seen.OFFLINE -> p.dim
        Seen.UNREACHABLE -> p.warn
        Seen.UNKNOWN -> p.surface3
    }
    val breathe = if (seen == Seen.ONLINE && !Motion.reduced) {
        val t = rememberInfiniteTransition(label = "dot")
        t.animateFloat(0.35f, 0f, infiniteRepeatable(tween(1600, easing = Motion.Spring), RepeatMode.Restart), label = "halo").value
    } else 0f
    Box(Modifier.size(14.dp), contentAlignment = Alignment.Center) {
        if (breathe > 0f) {
            Box(
                Modifier.size(14.dp)
                    .graphicsLayer { scaleX = 1.6f - breathe * 1.7f; scaleY = scaleX; alpha = breathe * 2f }
                    .clip(CircleShape).background(color),
            )
        }
        Box(Modifier.size(8.dp).clip(CircleShape).background(color))
    }
}
