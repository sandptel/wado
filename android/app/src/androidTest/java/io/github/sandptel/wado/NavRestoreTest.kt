package io.github.sandptel.wado

import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** WebView defect 6: the page the person was on survives the activity being recreated. */
@RunWith(AndroidJUnit4::class)
class NavRestoreTest {
    @get:Rule val rule = createAndroidComposeRule<MainActivity>()

    private fun openAppearance() {
        rule.onNodeWithTag("home").performScrollToNode(hasText("Appearance"))
        rule.onNodeWithText("Appearance").performClick()
        rule.waitUntil(5000) { rule.onAllNodes(hasContentDescription("Back")).fetchSemanticsNodes().isNotEmpty() }
    }

    @Test fun route_survives_recreate() {
        openAppearance()
        rule.activityRule.scenario.recreate()
        rule.waitForIdle()
        rule.onNodeWithText("Text size").assertExists()
        rule.onNodeWithContentDescription("Back").assertExists()
    }

    @Test fun back_from_a_page_returns_home() {
        openAppearance()
        rule.activityRule.scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }
        rule.waitUntil(5000) { rule.onAllNodes(hasText("On this phone")).fetchSemanticsNodes().isNotEmpty() }
    }
}
