package io.github.sandptel.wado.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import io.github.sandptel.wado.R

/** DESIGN.md's colour roles, from the user's scheme ([Base16]). */
@Immutable
data class Palette(
    val ground: Color,
    val surface1: Color,
    val surface2: Color,
    val surface3: Color,
    val line: Color,
    val bright: Color,
    val text: Color,
    val dim: Color,
    val accent: Color,
    val onAccent: Color,
    val good: Color,
    val warn: Color,
    val bad: Color,
    val light: Boolean,
    /** A computer's disc per accent slot (base08..base0F), softened into the ground. */
    val discs: List<Color> = emptyList(),
) {
    /** The disc for accent slot `hue` (0..7). */
    fun disc(hue: Int): Color = discs.getOrElse(hue.mod(8)) { surface2 }

    companion object {
        fun of(s: Base16) = Palette(
            ground = Color(s.ground), surface1 = Color(s.surface1), surface2 = Color(s.surface2),
            surface3 = Color(s.surface3), line = Color(s.line), bright = Color(s.bright),
            text = Color(s.text), dim = Color(s.dim), accent = Color(s.accent),
            onAccent = Color(s.onAccent), good = Color(s.good), warn = Color(s.warn), bad = Color(s.bad),
            light = s.light,
            discs = (0x8..0xF).map { Color(Base16.mix(s.ground, s.base(it), 0.30f)) },
        )
    }
}

/** The faces: Bricolage Grotesque for display, Figtree for body, JetBrains Mono for numbers. */
object Faces {
    private fun variable(res: Int, weight: Int, opsz: Float? = null) = Font(
        res,
        weight = FontWeight(weight),
        variationSettings = FontVariation.Settings(
            *listOfNotNull(FontVariation.weight(weight), opsz?.let { FontVariation.Setting("opsz", it) }).toTypedArray(),
        ),
    )

    val Display = FontFamily(
        variable(R.font.bricolage_grotesque, 500, 24f),
        variable(R.font.bricolage_grotesque, 600, 24f),
        variable(R.font.bricolage_grotesque, 700, 48f),
    )
    val Body = FontFamily(
        variable(R.font.figtree, 400),
        variable(R.font.figtree, 500),
        variable(R.font.figtree, 600),
        variable(R.font.figtree, 700),
    )
    val Mono = FontFamily(variable(R.font.jetbrains_mono, 400), variable(R.font.jetbrains_mono, 600))
}

/** DESIGN.md's type scale. */
object Type {
    val display = TextStyle(fontFamily = Faces.Display, fontWeight = FontWeight.Bold, fontSize = 48.sp, lineHeight = 48.sp, letterSpacing = (-0.045).em)
    val heroName = TextStyle(fontFamily = Faces.Display, fontWeight = FontWeight.Bold, fontSize = 30.sp, lineHeight = 31.sp, letterSpacing = (-0.025).em)
    val headline = TextStyle(fontFamily = Faces.Display, fontWeight = FontWeight.Bold, fontSize = 20.sp, letterSpacing = (-0.01).em)
    val numeral = TextStyle(fontFamily = Faces.Display, fontWeight = FontWeight.SemiBold, fontSize = 17.sp, lineHeight = 19.sp, fontFeatureSettings = "tnum")
    val title = TextStyle(fontFamily = Faces.Body, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
    val body = TextStyle(fontFamily = Faces.Body, fontWeight = FontWeight.Normal, fontSize = 14.sp, lineHeight = 20.sp)
    val small = TextStyle(fontFamily = Faces.Body, fontWeight = FontWeight.Normal, fontSize = 12.sp)
    val label = TextStyle(fontFamily = Faces.Body, fontWeight = FontWeight.SemiBold, fontSize = 11.sp, letterSpacing = 0.1.em)
    val mono = TextStyle(fontFamily = Faces.Mono, fontWeight = FontWeight.Normal, fontSize = 12.sp)
}

/** DESIGN.md's radii. */
object Radius {
    val r1 = 10.dp
    val r2 = 16.dp
    val r3 = 22.dp
    val r4 = 28.dp
}

val LocalPalette = staticCompositionLocalOf { Palette.of(Base16.DEFAULT) }

@Composable
fun WadoTheme(scheme: Base16, content: @Composable () -> Unit) {
    val p = remember(scheme) { Palette.of(scheme) }
    // Material 3 is plumbing only (text fields, ripples): it is fed the same roles.
    val m3 = if (p.light) {
        lightColorScheme(primary = p.accent, onPrimary = p.onAccent, background = p.ground, surface = p.surface1, onSurface = p.bright, onBackground = p.bright, error = p.bad)
    } else {
        darkColorScheme(primary = p.accent, onPrimary = p.onAccent, background = p.ground, surface = p.surface1, onSurface = p.bright, onBackground = p.bright, error = p.bad)
    }
    CompositionLocalProvider(LocalPalette provides p) {
        MaterialTheme(colorScheme = m3, content = content)
    }
}
