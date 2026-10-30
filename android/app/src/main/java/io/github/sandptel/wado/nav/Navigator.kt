package io.github.sandptel.wado.nav

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.navigation3.runtime.NavBackStack
import androidx.navigation3.runtime.NavKey

/**
 * The rules of the back stack, for the activity and the screens alike. The stack itself is made in
 * composition (`rememberNavBackStack`, so it is saved); anything asked before then waits and is
 * applied when it arrives. Home is always at the bottom.
 */
class Navigator {
    private var stack: NavBackStack<NavKey>? = null
    private val pending = mutableListOf<Dest>()

    /** The page on top, for the activity's decisions (null before the first composition). */
    var top by mutableStateOf<Dest?>(null)
        private set

    fun attach(s: NavBackStack<NavKey>) {
        if (stack === s) return
        stack = s
        if (s.isEmpty()) s.add(Dest.Home)
        pending.forEach { push(s, it) }
        pending.clear()
        sync()
    }

    fun sync() {
        top = stack?.lastOrNull() as? Dest
    }

    fun go(d: Dest) {
        val s = stack ?: return run { pending += d }
        push(s, d)
        sync()
    }

    private fun push(s: NavBackStack<NavKey>, d: Dest) {
        when {
            d == Dest.Home -> while (s.size > 1) s.removeAt(s.lastIndex)
            s.lastOrNull() == d -> {}
            // One web layer: a new door replaces the last one rather than stacking on it.
            d is Dest.Web && s.lastOrNull() is Dest.Web -> s[s.lastIndex] = d
            else -> s.add(d)
        }
    }

    /** Back one page; false at home. */
    fun pop(): Boolean {
        val s = stack ?: return false
        if (s.size <= 1) return false
        s.removeAt(s.lastIndex)
        sync()
        return true
    }

    fun home() = go(Dest.Home)
}
