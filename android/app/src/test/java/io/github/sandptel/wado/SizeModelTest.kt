package io.github.sandptel.wado

import androidx.compose.ui.unit.Density
import io.github.sandptel.wado.ui.phone.Size
import io.github.sandptel.wado.ui.phone.SizeModel
import org.junit.Assert.assertEquals
import org.junit.Test

class SizeModelTest {
    @Test fun compact_keeps_44dp_on_glass() {
        for ((_, ui) in SizeModel.UI_STEPS) {
            val t = SizeModel.tapTarget(Size(ui = ui))
            // In scope, a dp is `ui` dp on the glass.
            assertEquals("ui $ui", 44f, t.value * ui, 0.001f)
        }
    }

    @Test fun follow_uses_system_scale_only_when_on() {
        val base = Density(2.75f, 1.3f)
        val follow = SizeModel.density(base, Size(text = 0.9f, follow = true, ui = 1.1f), 1.3f)
        assertEquals(1.3f, follow.fontScale, 0f)
        assertEquals(2.75f * 1.1f, follow.density, 0.0001f)
        val chosen = SizeModel.density(base, Size(text = 0.9f, follow = false, ui = 1f), 1.3f)
        assertEquals(0.9f, chosen.fontScale, 0f)
        assertEquals(2.75f, chosen.density, 0f)
    }

    @Test fun text_range_is_85_to_150_in_5s() {
        val s = SizeModel.TEXT_STEPS
        assertEquals(0.85f, s.first(), 0.0001f)
        assertEquals(1.5f, s.last(), 0.0001f)
        assertEquals(14, s.size)
        s.zipWithNext().forEach { (a, b) -> assertEquals(0.05f, b - a, 0.0001f) }
        assertEquals(1.15f, SizeModel.snapText(1.13f), 0.0001f)
    }
}
