package io.github.sandptel.wado.ui.home

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.core.LinkModel
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Motion
import io.github.sandptel.wado.ui.theme.Palette
import io.github.sandptel.wado.ui.theme.Type
import uniffi.wado_mobile_core.LinkTone

/**
 * The link's plain words with their tone dot: a ripple while live, a breathe while connecting or
 * reconnecting (2.4 s, the ambient exception), still when offline, a warning colour when refused.
 * Read out politely when they change.
 */
@Composable
fun LinkWords(link: LinkModel, suffix: String = "", modifier: Modifier = Modifier) {
    val p = LocalPalette.current
    Row(
        modifier.semantics { liveRegion = LiveRegionMode.Polite },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ToneDot(link.tone)
        Spacer(Modifier.width(6.dp))
        Text(link.words + suffix, style = Type.small, color = p.dim, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

fun toneColor(p: Palette, tone: LinkTone): Color = when (tone) {
    LinkTone.LIVE -> p.good
    LinkTone.BUSY -> p.accent
    LinkTone.STILL -> p.dim
    LinkTone.BAD -> p.warn
}

@Composable
fun ToneDot(tone: LinkTone) {
    val p = LocalPalette.current
    val color = toneColor(p, tone)
    val moving = !Motion.reduced && (tone == LinkTone.LIVE || tone == LinkTone.BUSY)
    val t = if (moving) {
        val inf = rememberInfiniteTransition(label = "tone")
        if (tone == LinkTone.LIVE) {
            // A ripple: a ring leaves the dot and fades, on a clock.
            inf.animateFloat(0f, 1f, infiniteRepeatable(tween(2400, easing = LinearEasing), RepeatMode.Restart), label = "ripple").value
        } else {
            inf.animateFloat(0.35f, 1f, infiniteRepeatable(tween(1200, easing = Motion.Spring), RepeatMode.Reverse), label = "breathe").value
        }
    } else 0f
    Box(Modifier.size(14.dp), contentAlignment = Alignment.Center) {
        if (moving && tone == LinkTone.LIVE) {
            Box(
                Modifier.size(14.dp)
                    .graphicsLayer { scaleX = 0.6f + t; scaleY = scaleX; alpha = (1f - t) * 0.5f }
                    .clip(CircleShape).background(color),
            )
        }
        Box(
            Modifier.size(8.dp)
                .graphicsLayer { alpha = if (moving && tone == LinkTone.BUSY) t else 1f }
                .clip(CircleShape).background(color),
        )
    }
}
