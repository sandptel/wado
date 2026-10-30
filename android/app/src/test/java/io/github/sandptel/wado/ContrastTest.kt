package io.github.sandptel.wado

import io.github.sandptel.wado.ui.theme.Base16
import io.github.sandptel.wado.ui.theme.Schemes
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow

/**
 * The role mixer keeps text readable (WCAG AA) on three schemes from schemes.json, dark and light:
 * body text (bright, text) at 4.5:1 on every surface it sits on; the dim role, used for secondary
 * lines, at 3:1 (AA for large text and non-text marks).
 */
class ContrastTest {
    private fun lum(c: Int): Double {
        fun ch(v: Int): Double = (v / 255.0).let { if (it <= 0.03928) it / 12.92 else ((it + 0.055) / 1.055).pow(2.4) }
        return 0.2126 * ch((c shr 16) and 255) + 0.7152 * ch((c shr 8) and 255) + 0.0722 * ch(c and 255)
    }

    private fun ratio(a: Int, b: Int): Double {
        val (x, y) = lum(a) to lum(b)
        return (max(x, y) + 0.05) / (min(x, y) + 0.05)
    }

    private fun schemes(): Map<String, Base16> {
        var d = File("").absoluteFile
        while (!File(d, "crates/client/src/schemes.json").exists()) d = d.parentFile ?: error("no repo root")
        return Schemes.parse(File(d, "crates/client/src/schemes.json").readText()).toMap()
    }

    @Test fun roles_meet_aa_on_three_schemes() {
        val all = schemes()
        val failures = mutableListOf<String>()
        for (name in listOf("default-dark", "catppuccin-latte", "nord")) {
            val s = all.getValue(name)
            for ((bgName, bg) in listOf("ground" to s.ground, "surface1" to s.surface1, "surface2" to s.surface2)) {
                for ((role, fg, min) in listOf(Triple("bright", s.bright, 4.5), Triple("text", s.text, 4.5), Triple("dim", s.dim, 3.0))) {
                    val r = ratio(fg, bg)
                    if (r < min) failures += "$name: $role on $bgName = ${"%.2f".format(r)} < $min"
                }
            }
        }
        assertTrue(failures.joinToString("\n"), failures.isEmpty())
    }
}
