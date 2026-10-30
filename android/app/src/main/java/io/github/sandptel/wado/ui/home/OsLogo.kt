package io.github.sandptel.wado.ui.home

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.R
import io.github.sandptel.wado.ui.theme.LocalPalette

/**
 * A computer's OS logo (`scripts/os-logos.mjs`, Simple Icons, CC0) in a disc of its hue. Monochrome
 * in the `bright` role — never the brand's colour (the Sixteen Slots Rule).
 */
object OsLogo {
    private val cache = HashMap<String, Int>()

    /** The drawable for a core slug; Tux for one this build has no drawable for. */
    fun drawable(slug: String): Int = cache.getOrPut(slug) {
        runCatching { R.drawable::class.java.getField("os_$slug").getInt(null) }.getOrDefault(R.drawable.os_linux)
    }
}

@Composable
fun OsLogoDisc(slug: String, hue: Int, size: Dp = 46.dp) {
    val p = LocalPalette.current
    Box(Modifier.size(size).clip(CircleShape).background(p.disc(hue)), contentAlignment = Alignment.Center) {
        Image(
            painterResource(OsLogo.drawable(slug)),
            contentDescription = null,
            colorFilter = ColorFilter.tint(p.bright),
            modifier = Modifier.size(size * 0.5f),
        )
    }
}
