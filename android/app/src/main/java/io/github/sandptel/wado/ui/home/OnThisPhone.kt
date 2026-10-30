package io.github.sandptel.wado.ui.home

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.nav.PhonePage
import io.github.sandptel.wado.ui.common.LineIcon
import io.github.sandptel.wado.ui.common.Pressable
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type

private val ROWS = listOf(
    Triple(PhonePage.Appearance, "Scheme, accent, text and interface size, motion", "palette"),
    Triple(PhonePage.Background, "Stay connected, clipboard, volume keys", "bell"),
    Triple(PhonePage.Diagnostics, "Each computer's link, the relay, versions", "pulse"),
    Triple(PhonePage.About, "Licences and source", "info"),
)

/** "On this phone": what belongs to this phone rather than to a computer, one row a page. */
@Composable
fun OnThisPhone(onPage: (PhonePage) -> Unit, modifier: Modifier = Modifier) {
    val p = LocalPalette.current
    Column(
        modifier.fillMaxWidth().widthIn(max = 720.dp).clip(RoundedCornerShape(Radius.r3)).background(p.surface1).padding(vertical = 4.dp),
    ) {
        ROWS.forEach { (page, sub, icon) ->
            Pressable({ onPage(page) }, Modifier.fillMaxWidth().heightIn(min = 56.dp), color = p.surface1, radius = Radius.r2, sink = 0.99f) {
                Row(Modifier.padding(horizontal = 14.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                    LineIcon(icon, p.bright, size = 20.dp)
                    Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f)) {
                        Text(page.title, style = Type.title, color = p.bright)
                        Text(sub, style = Type.small, color = p.dim)
                    }
                    LineIcon("right", p.dim, size = 18.dp)
                }
            }
        }
    }
}
