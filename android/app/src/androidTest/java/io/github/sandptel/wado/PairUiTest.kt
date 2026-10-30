package io.github.sandptel.wado

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.text.TextLayoutResult
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.sandptel.wado.core.PairModel
import io.github.sandptel.wado.ui.pair.CodeField
import io.github.sandptel.wado.ui.pair.PairStatus
import io.github.sandptel.wado.ui.phone.Size
import io.github.sandptel.wado.ui.phone.SizeScope
import io.github.sandptel.wado.ui.theme.Base16
import io.github.sandptel.wado.ui.theme.WadoTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.wado_mobile_core.PairProgress

/** The pairing screens' parts. "Done returns home with the new card" is the reducer's (CoreStateTest). */
@RunWith(AndroidJUnit4::class)
class PairUiTest {
    @get:Rule val rule = createComposeRule()

    @Test fun typed_code_shows_four_words_and_waits() {
        rule.setContent {
            WadoTheme(Base16.DEFAULT) {
                PairStatus(PairModel(1, PairProgress.CheckWords(listOf("desert", "badger", "torch", "shell"))), "") {}
            }
        }
        rule.onNodeWithText("desert  badger  torch  shell").assertExists()
        rule.onNodeWithText("Check these match the computer's screen, then say yes there.").assertExists()
    }

    @Test fun failure_reason_is_plain_with_try_again() {
        var retried = false
        rule.setContent {
            WadoTheme(Base16.DEFAULT) {
                PairStatus(PairModel(1, PairProgress.Failed("That code didn't work — check it.")), "") { retried = true }
            }
        }
        rule.onNodeWithText("That code didn't work — check it.").assertExists()
        rule.onNodeWithText("Try again").assertExists()
    }

    @Test fun code_field_text_is_16sp_on_glass() {
        val base = rule.density
        rule.setContent {
            WadoTheme(Base16.DEFAULT) {
                // The smallest sizes there are: compact interface, 85 % text.
                SizeScope(Size(text = 0.85f, follow = false, ui = 0.9f)) {
                    var v by remember { mutableStateOf("") }
                    CodeField(v, { v = it })
                }
            }
        }
        rule.onNodeWithTag("code").performTextInput("K7QF2MXP")
        val layouts = mutableListOf<TextLayoutResult>()
        rule.onNodeWithTag("code").fetchSemanticsNode().config[SemanticsActions.GetTextLayoutResult].action?.invoke(layouts)
        val style = layouts.first().layoutInput
        val px = with(style.density) { style.style.fontSize.toPx() }
        val glass16 = 16f * base.density * base.fontScale
        assertTrue("code text is $px px, 16 sp on the glass is $glass16 px", px >= glass16 - 0.5f)
    }
}
