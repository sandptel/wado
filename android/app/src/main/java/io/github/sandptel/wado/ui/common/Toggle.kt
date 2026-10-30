package io.github.sandptel.wado.ui.common

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.device.Haptics
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Motion
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type

/** A rounded group of rows on surface-1. */
@Composable
fun Group(content: @Composable () -> Unit) {
    val p = LocalPalette.current
    Column(
        Modifier.fillMaxWidth().widthIn(max = 720.dp).clip(RoundedCornerShape(Radius.r3)).background(p.surface1).padding(vertical = 4.dp),
    ) { content() }
}

/** A nav-row with DESIGN.md's switch (52 × 32, accent when on): the knob pops across. */
@Composable
fun Toggle(title: String, sub: String, initial: Boolean, onChange: (Boolean) -> Unit) {
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
        Modifier.fillMaxWidth().semantics { role = Role.Switch; stateDescription = if (on) "On" else "Off" },
        color = p.surface1,
        radius = Radius.r2,
        sink = 0.99f,
    ) {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(title, style = Type.title, color = p.bright)
                if (sub.isNotEmpty()) Text(sub, style = Type.small, color = p.dim)
            }
            Spacer(Modifier.width(12.dp))
            Box(Modifier.size(width = 52.dp, height = 32.dp).clip(CircleShape).background(track)) {
                Box(Modifier.offset(x = knob, y = 3.dp).size(26.dp).clip(CircleShape).background(if (on) p.onAccent else p.bright))
            }
        }
    }
}
