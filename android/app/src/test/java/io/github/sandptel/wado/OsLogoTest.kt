package io.github.sandptel.wado

import io.github.sandptel.wado.ui.home.OsLogo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Every slug the core can name has a drawable (`scripts/os-logos.mjs`). The JVM test cannot load
 * the phone's .so, so it reads the core's table the way the script does — the same source as
 * `os_logo_slugs()`.
 */
class OsLogoTest {
    private fun root(): File {
        var d = File("").absoluteFile
        while (!File(d, "crates/client-core/src/os_logo.rs").exists()) d = d.parentFile ?: error("no repo root")
        return d
    }

    private fun slugs(): List<String> {
        val src = File(root(), "crates/client-core/src/os_logo.rs").readText()
        val table = src.substring(src.indexOf("const IDS"), src.indexOf("];"))
        return (Regex("""\("[^"]+",\s*"([^"]+)"\)""").findAll(table).map { it.groupValues[1] } + "linux").distinct().toList()
    }

    @Test fun every_core_slug_has_a_drawable() {
        val all = slugs()
        assertTrue(all.size > 10)
        for (s in all) {
            assertTrue("os_$s.xml", File(root(), "android/app/src/main/res/drawable/os_$s.xml").exists())
            val id = OsLogo.drawable(s)
            if (s != "linux") assertNotEquals("$s resolves to its own drawable", OsLogo.drawable("linux"), id)
        }
    }

    @Test fun unknown_slug_is_tux() = assertEquals(OsLogo.drawable("linux"), OsLogo.drawable("no-such-os"))
}
