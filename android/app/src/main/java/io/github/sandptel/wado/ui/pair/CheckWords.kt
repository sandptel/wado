package io.github.sandptel.wado.ui.pair

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Type

/** How long the computer waits for someone to confirm the words (`gate/pairing.rs`). */
const val CHECK_WORDS_MS = 120_000

/**
 * A typed code's four check words, large and in mono, with the two minutes the computer gives as
 * a linear clock (a clock, not a curve). The person compares them with the computer's screen and
 * says yes there.
 */
@Composable
fun CheckWords(words: List<String>) {
    val p = LocalPalette.current
    val left = remember(words) { Animatable(1f) }
    LaunchedEffect(words) { left.animateTo(0f, tween(CHECK_WORDS_MS, easing = LinearEasing)) }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(
            words.joinToString("  "),
            style = Type.mono.copy(fontSize = 26.sp),
            color = p.bright,
            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
        )
        Text("Check these match the computer's screen, then say yes there.", style = Type.body, color = p.text)
        Box(Modifier.fillMaxWidth().height(4.dp).clip(CircleShape).background(p.surface3)) {
            Box(Modifier.fillMaxWidth(left.value).fillMaxHeight().clip(CircleShape).background(p.accent))
        }
    }
}
