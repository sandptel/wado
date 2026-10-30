package io.github.sandptel.wado.ui.common

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Type

/** A page's top: Back (48 dp) and its title. */
@Composable
fun TopBar(title: String, onBack: () -> Unit) {
    val p = LocalPalette.current
    Row(Modifier.fillMaxWidth().widthIn(max = 720.dp).padding(bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Pressable(onBack, Modifier.size(48.dp).semanticsLabel("Back"), color = p.ground, radius = 24.dp, sink = 0.9f) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { LineIcon("back", p.bright) }
        }
        Spacer(Modifier.width(6.dp))
        Text(title, style = Type.heroName, color = p.bright, modifier = Modifier.semantics { heading() })
    }
}
