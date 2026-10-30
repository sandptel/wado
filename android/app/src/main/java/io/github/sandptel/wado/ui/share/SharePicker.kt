package io.github.sandptel.wado.ui.share

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.bridge.Computer
import io.github.sandptel.wado.bridge.Computers
import io.github.sandptel.wado.online.OnlineProbe
import io.github.sandptel.wado.share.SharePayload
import io.github.sandptel.wado.ui.common.LineIcon
import io.github.sandptel.wado.ui.common.Pressable
import io.github.sandptel.wado.ui.common.WadoButton
import io.github.sandptel.wado.ui.home.StatusDot
import io.github.sandptel.wado.ui.home.status
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Motion
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type
import kotlinx.coroutines.launch

/** Where a share is, from the sheet's point of view. */
sealed interface ShareState {
    data object Choosing : ShareState
    data class Sending(val to: Computer) : ShareState
    data class Done(val to: Computer) : ShareState
    data class Failed(val to: Computer, val why: String) : ShareState
}

/**
 * "Send to computer": a sheet over whatever wado was showing, rising on the sheet curve. It
 * names what is being sent, lists the computers with their live status, and then says plainly
 * what happened — files land in the computer's Downloads (the transfer list shows progress),
 * links open in its session, text goes to its clipboard.
 */
@Composable
fun SharePicker(
    payload: SharePayload,
    computers: Computers,
    state: ShareState,
    onPick: (Computer) -> Unit,
    onClose: () -> Unit,
) {
    val p = LocalPalette.current
    val scope = rememberCoroutineScope()
    val rise = remember { Animatable(if (Motion.reduced) 1f else 0f) }
    LaunchedEffect(Unit) { rise.animateTo(1f, Motion.spring(Motion.DUR_SHEET)) }
    val close: () -> Unit = {
        scope.launch {
            rise.animateTo(0f, Motion.spring(Motion.DUR_SHEET_EXIT))
            onClose()
        }
    }
    var seen by remember { mutableStateOf<Map<String, OnlineProbe.Seen>>(emptyMap()) }
    LaunchedEffect(computers.list) { seen = OnlineProbe.probeAll(computers.list) }

    Box(Modifier.fillMaxSize()) {
        Box(
            Modifier.fillMaxSize()
                .graphicsLayer { alpha = rise.value }
                .background(Color.Black.copy(alpha = 0.55f))
                .clickable(remember { MutableInteractionSource() }, null) { if (state !is ShareState.Sending) close() },
        )
        Column(
            Modifier.align(Alignment.BottomCenter)
                .widthIn(max = 720.dp).fillMaxWidth()
                .graphicsLayer { translationY = (1f - rise.value) * size.height }
                .clip(RoundedCornerShape(topStart = Radius.r4, topEnd = Radius.r4))
                .background(p.surface1)
                .windowInsetsPadding(WindowInsets.navigationBars)
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Box(Modifier.align(Alignment.CenterHorizontally).size(width = 36.dp, height = 4.dp).clip(CircleShape).background(p.surface3))
            Text("Send ${payload.describe()} to…", style = Type.headline, color = p.bright)
            when (state) {
                ShareState.Choosing -> {
                    if (computers.list.isEmpty()) {
                        Text("Pair a computer first — then share to it from any app.", style = Type.body, color = p.dim)
                    }
                    computers.list.forEach { c ->
                        val s = seen[c.key] ?: OnlineProbe.Seen.UNKNOWN
                        Pressable({ onPick(c) }, Modifier.fillMaxWidth().height(60.dp), radius = Radius.r2) {
                            Row(Modifier.fillMaxSize().padding(horizontal = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                                LineIcon("monitor", p.bright)
                                Spacer(Modifier.width(12.dp))
                                Column(Modifier.weight(1f)) {
                                    Text(c.title, style = Type.title, color = p.bright, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        StatusDot(s)
                                        Text(status(s), style = Type.small, color = p.dim)
                                    }
                                }
                                LineIcon("right", p.dim, size = 18.dp)
                            }
                        }
                    }
                }
                is ShareState.Sending -> Text("Sending to ${state.to.title}…", style = Type.body, color = p.dim)
                is ShareState.Done -> Outcome(true, doneWords(payload, state.to))
                is ShareState.Failed -> Outcome(false, "It didn't reach ${state.to.title}: ${state.why}")
            }
            if (state !is ShareState.Sending) WadoButton(if (state is ShareState.Done) "Done" else "Cancel", close, Modifier.fillMaxWidth())
        }
    }
}

private fun doneWords(payload: SharePayload, to: Computer) = buildList {
    if (payload.files.isNotEmpty()) add("Sending to Downloads on ${to.title} — the transfer list shows it.")
    if (payload.links.isNotEmpty()) add("Opened on ${to.title}.")
    if (payload.files.isEmpty() && payload.links.isEmpty()) add("On ${to.title}'s clipboard.")
}.joinToString(" ")

@Composable
private fun Outcome(ok: Boolean, words: String) {
    val p = LocalPalette.current
    val pop = remember { Animatable(if (Motion.reduced) 1f else 0.6f) }
    LaunchedEffect(Unit) { pop.animateTo(1f, Motion.pop()) }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(
            Modifier.size(36.dp).graphicsLayer { scaleX = pop.value; scaleY = pop.value }.clip(CircleShape)
                .background(if (ok) p.good else p.bad),
            contentAlignment = Alignment.Center,
        ) { LineIcon(if (ok) "check" else "x", p.ground, size = 20.dp) }
        Spacer(Modifier.width(12.dp))
        Text(words, style = Type.body, color = p.text)
    }
}
