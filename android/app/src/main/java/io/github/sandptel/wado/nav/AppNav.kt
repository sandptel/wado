package io.github.sandptel.wado.nav

import androidx.compose.animation.ContentTransform
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.navigation3.runtime.NavEntry
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.ui.NavDisplay
import io.github.sandptel.wado.ui.theme.Motion

/**
 * The app's pages on one saved back stack. Pushes come in from the right on the spring (480 ms,
 * the page behind drifting −24 % and fading); pops run the other way quicker (340 ms); predictive
 * back scrubs the pop with the gesture. `onBack` decides what Back means on the top page (the web
 * layer asks the page first).
 */
@Composable
fun AppNav(
    nav: Navigator,
    onBack: (Dest) -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable (Dest) -> Unit,
) {
    val stack = rememberNavBackStack(Dest.Home)
    nav.attach(stack)
    LaunchedEffect(stack) { snapshotFlow { stack.toList() }.collect { nav.sync() } }
    NavDisplay(
        backStack = stack,
        modifier = modifier,
        onBack = { (stack.lastOrNull() as? Dest)?.let(onBack) },
        transitionSpec = { push() },
        popTransitionSpec = { pop() },
        predictivePopTransitionSpec = { pop() },
        entryProvider = { key -> NavEntry(key) { content(key as Dest) } },
    )
}

private fun push(): ContentTransform {
    if (Motion.reduced) return fadeIn(Motion.spring(0)) togetherWith fadeOut(Motion.spring(0))
    return (slideInHorizontally(Motion.spring(Motion.DUR_SHEET)) { it } + fadeIn(Motion.spring(Motion.DUR))) togetherWith
        (slideOutHorizontally(Motion.spring(Motion.DUR_SHEET)) { -it * 24 / 100 } + fadeOut(Motion.exit()))
}

private fun pop(): ContentTransform {
    if (Motion.reduced) return fadeIn(Motion.spring(0)) togetherWith fadeOut(Motion.spring(0))
    return (slideInHorizontally(Motion.spring(Motion.DUR_POP)) { -it * 24 / 100 } + fadeIn(Motion.spring(Motion.DUR))) togetherWith
        (slideOutHorizontally(Motion.spring(Motion.DUR_POP)) { it } + fadeOut(Motion.exit()))
}
