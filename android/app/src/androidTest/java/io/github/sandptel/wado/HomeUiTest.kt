package io.github.sandptel.wado

import androidx.compose.ui.test.assertHasNoClickAction
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.sandptel.wado.core.CardModel
import io.github.sandptel.wado.core.HomeModel
import io.github.sandptel.wado.core.LinkModel
import io.github.sandptel.wado.core.RunningModel
import io.github.sandptel.wado.nav.PhonePage
import io.github.sandptel.wado.ui.home.HomeActions
import io.github.sandptel.wado.ui.home.HomeScreen
import io.github.sandptel.wado.ui.theme.Base16
import io.github.sandptel.wado.ui.theme.WadoTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.wado_mobile_core.LinkTone

@RunWith(AndroidJUnit4::class)
class HomeUiTest {
    @get:Rule val rule = createComposeRule()

    private val none = object : HomeActions {
        override fun open(card: CardModel) {}
        override fun play(card: CardModel) {}
        override fun door(card: CardModel, go: String) {}
        override fun pair() {}
        override fun running(r: RunningModel) {}
        override fun phone(page: PhonePage) {}
        override fun web() {}
    }

    private fun card(online: Boolean, sessions: Int = 0) = CardModel(
        key = "1", name = "linux-box", remoteId = "111222333", relayUrl = "https://r",
        osLine = "NixOS 26.11 · Hyprland", logo = "archlinux", hue = 2,
        link = if (online) LinkModel("open", "Online", LinkTone.LIVE, "Open", emptyList())
        else LinkModel("offline", "Offline · seen 2 h ago", LinkTone.STILL, "Offline", emptyList()),
        sessions = sessions, webKey = "w1",
    )

    private fun show(m: HomeModel) = rule.setContent { WadoTheme(Base16.DEFAULT) { HomeScreen(m, none) } }

    @Test fun card_shows_logo_name_os_and_words() {
        show(HomeModel(computers = listOf(card(online = true, sessions = 1))))
        rule.onNodeWithText("linux-box").assertExists()
        rule.onNodeWithText("NixOS 26.11 · Hyprland").assertExists()
        rule.onNodeWithText("Online · 1 session").assertExists()
        rule.onNodeWithContentDescription("Desktop on linux-box — a session is running").assertExists()
    }

    @Test fun offline_card_play_is_disabled_with_reason() {
        show(HomeModel(computers = listOf(card(online = false))))
        rule.onNodeWithContentDescription("Desktop — linux-box is offline").assertExists().assertHasNoClickAction()
    }

    @Test fun legacy_card_offers_pair_the_app() {
        show(HomeModel(computers = listOf(CardModel("web:w2", "Old laptop", "444", "https://r", needsAppPairing = true, webKey = "w2"))))
        rule.onNodeWithText("Pair the app").assertExists()
        rule.onNodeWithText("Known to the web part only").assertExists()
    }

    @Test fun running_section_names_the_computer() {
        show(
            HomeModel(
                computers = listOf(card(online = true, sessions = 1)),
                running = listOf(RunningModel("1", "linux-box", "1:1", "firefox", 2, "Pixel", 1)),
            ),
        )
        rule.onNodeWithText("firefox +2").assertExists()
        rule.onNodeWithText("on linux-box · sized for Pixel").assertExists()
    }

    @Test fun targets_are_44dp() {
        show(HomeModel(computers = listOf(card(online = true), card(online = false).copy(key = "2"))))
        val density = rule.density.density
        val small = rule.onAllNodes(hasClickAction()).fetchSemanticsNodes().filter {
            it.size.width / density < 43.5f || it.size.height / density < 43.5f
        }
        assertTrue("targets under 44 dp: ${small.map { it.config }}", small.isEmpty())
    }
}
