package io.github.sandptel.wado.ui.pad

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.bridge.BridgeProtocol
import io.github.sandptel.wado.bridge.Computer
import io.github.sandptel.wado.bridge.Hub
import io.github.sandptel.wado.device.Haptics
import io.github.sandptel.wado.pad.PadLink
import io.github.sandptel.wado.pad.TapFeel
import io.github.sandptel.wado.ui.common.LineIcon
import io.github.sandptel.wado.ui.common.Pressable
import io.github.sandptel.wado.ui.common.semanticsLabel
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Motion
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type

/**
 * The native trackpad: the whole screen a touchpad for the computer, held landscape (a real
 * orientation lock), the screen kept on. Tracking is 1:1 — a ring is where the finger is, this
 * frame — and taps tick in the hand. The frames are the Rust core's (PadLink); the computer's
 * libinput reads them as its own touchpad, so taps, scrolls and pinches are its.
 *
 * The web pad's extras — phone gestures, the action rail, keys, its settings — are one tap away
 * ("More"), until they are ported.
 */
@Composable
fun PadScreen(computer: Computer, modes: PadModes, onMore: () -> Unit, onBack: () -> Unit) {
    val p = LocalPalette.current
    val view = LocalView.current
    val link = remember(computer.key) { PadLink(computer.key) }
    var state by remember { mutableStateOf("off") }
    var problem by remember { mutableStateOf<String?>(null) }
    val fingers = remember { mutableStateMapOf<Long, Offset>() }
    val tap = remember { TapFeel() }

    DisposableEffect(link) {
        modes.enter()
        link.start()
        onDispose {
            link.stop()
            modes.leave()
        }
    }
    LaunchedEffect(link) {
        Hub.events.collect { (t, o) ->
            when (t) {
                BridgeProtocol.PAD_STATE -> {
                    state = o.optString("state")
                    link.state(state)
                }
                BridgeProtocol.PAD_MSG -> o.optJSONObject("msg")?.let { m ->
                    if (m.optString("k") == "err") {
                        problem = m.optString("message")
                        if (m.optBoolean("fatal")) Haptics.perform(view, "reject")
                    }
                }
            }
        }
    }
    // The pacer's clock: relayed ticks, repeats and resting re-sends fall due between events.
    LaunchedEffect(link) {
        while (true) withFrameNanos { link.poll() }
    }

    Box(Modifier.fillMaxSize().background(p.ground)) {
        Box(
            Modifier.fillMaxSize()
                .onSizeChanged { link.resize(it.width, it.height) }
                .pointerInput(link) {
                    awaitPointerEventScope {
                        while (true) {
                            val e = awaitPointerEvent()
                            for (c in e.changes) {
                                val id = c.id.value
                                when {
                                    c.pressed && !c.previousPressed -> {
                                        fingers[id] = c.position
                                        link.down(id, c.position.x, c.position.y)
                                        tap.down(id, c.position, c.uptimeMillis, fingers.size)
                                    }
                                    c.pressed -> {
                                        fingers[id] = c.position
                                        link.move(id, c.position.x, c.position.y)
                                    }
                                    c.previousPressed -> {
                                        fingers.remove(id)
                                        link.up(id)
                                        if (tap.up(id, c.position, c.uptimeMillis, fingers.isEmpty())) Haptics.perform(view, "tap")
                                    }
                                }
                                c.consume()
                            }
                            if (e.type == PointerEventType.Exit && fingers.isNotEmpty()) {
                                fingers.clear()
                                link.liftAll()
                            }
                        }
                    }
                },
        ) {
            Rings(fingers)
        }
        // Chrome: back, the computer, the link's honest state, More.
        Row(
            Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Pressable(onBack, Modifier.size(44.dp).semanticsLabel("Back"), color = p.surface1, radius = 22.dp, sink = 0.9f) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { LineIcon("back", p.bright) }
            }
            Spacer(Modifier.width(10.dp))
            Column(Modifier.weight(1f)) {
                Text(computer.title, style = Type.title, color = p.bright)
                Text(stateWords(state), style = Type.small, color = if (state == "off") p.warn else p.dim)
            }
            Pressable(onMore, Modifier.height(40.dp), color = p.surface1, radius = Radius.r2) {
                Row(Modifier.padding(horizontal = 14.dp).height(40.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    LineIcon("kbd", p.bright, size = 18.dp)
                    Text("Keys & gestures", style = Type.small, color = p.bright)
                }
            }
        }
        problem?.let {
            Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.BottomCenter) {
                Text(
                    it,
                    style = Type.body,
                    color = p.bright,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.clip(RoundedCornerShape(Radius.r2)).background(p.surface2).padding(horizontal = 16.dp, vertical = 10.dp),
                )
            }
        }
    }
}

private fun stateWords(state: String) = when (state) {
    "direct" -> "Trackpad · direct"
    "relayed" -> "Trackpad · through the relay"
    else -> "Connecting…"
}

/** A ring under each finger, exactly where it is; it lands with a small pop. */
@Composable
private fun Rings(fingers: Map<Long, Offset>) {
    val p = LocalPalette.current
    val land = remember { Animatable(1f) }
    var count by remember { mutableStateOf(0) }
    LaunchedEffect(fingers.size) {
        if (fingers.size > count && !Motion.reduced) {
            land.snapTo(0.7f)
            land.animateTo(1f, Motion.pop(Motion.DUR))
        }
        count = fingers.size
    }
    val r = with(LocalDensity.current) { 34.dp.toPx() }
    Canvas(Modifier.fillMaxSize()) {
        // An empty pad says what it is, quietly.
        if (fingers.isEmpty()) {
            drawCircle(p.surface2, radius = r * 0.25f, center = center)
        }
        for (pos in fingers.values) {
            drawCircle(p.accent.copy(alpha = 0.16f), radius = r * land.value, center = pos)
            drawCircle(p.accent, radius = r * land.value, center = pos, style = Stroke(width = 3.dp.toPx()))
        }
    }
}

/** What the pad asks of the window while it is up. */
interface PadModes {
    fun enter()
    fun leave()
}
