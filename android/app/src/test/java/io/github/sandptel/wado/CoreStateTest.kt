package io.github.sandptel.wado

import io.github.sandptel.wado.bridge.Computer
import io.github.sandptel.wado.core.CoreState
import io.github.sandptel.wado.core.HomeModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.wado_mobile_core.ComputerInfo
import uniffi.wado_mobile_core.CoreEvent
import uniffi.wado_mobile_core.HostInfo
import uniffi.wado_mobile_core.LinkInfo
import uniffi.wado_mobile_core.LinkTone
import uniffi.wado_mobile_core.PairProgress
import uniffi.wado_mobile_core.SessionInfo

class CoreStateTest {
    private fun computer(key: String, rid: String = key, name: String = "box-$key") =
        ComputerInfo(key, name, rid, "https://relay.example", "", "linux", "", 0u)

    private fun two() = CoreState.reduce(HomeModel(), CoreEvent.Computers(listOf(computer("1"), computer("2"))))

    private fun link(kind: String, words: String) = LinkInfo(kind, words, LinkTone.LIVE, kind, emptyList(), "1:1")

    @Test fun link_event_updates_only_that_card() {
        val m = CoreState.reduce(two(), CoreEvent.Link("2", link("open", "Online")))
        assertEquals("Not connected", m.computers[0].link.words)
        assertEquals("Online", m.computers[1].link.words)
        assertTrue(m.computers[1].link.online)
        // A later computers list keeps what the link said.
        val again = CoreState.reduce(m, CoreEvent.Computers(listOf(computer("1"), computer("2"))))
        assertEquals("Online", again.computers[1].link.words)
    }

    @Test fun host_event_sets_logo_and_os_line() {
        val host = HostInfo("box", "NixOS 26.11 (Xantusia)", "nixos", "Hyprland", 5u, emptyList(), null, 0.5f, null, true)
        val m = CoreState.reduce(two(), CoreEvent.Host("1", host))
        assertEquals("nixos", m.computers[0].logo)
        assertEquals("NixOS 26.11 (Xantusia) · Hyprland", m.computers[0].osLine)
        assertEquals("linux", m.computers[1].logo)
    }

    @Test fun pair_done_adds_card_and_clears_pairing() {
        var m = CoreState.reduce(two(), CoreEvent.Pair(7u, PairProgress.Securing))
        assertEquals(7L, m.pairing?.op)
        m = CoreState.reduce(m, CoreEvent.Pair(7u, PairProgress.Done("3")))
        assertNull(m.pairing)
        assertEquals("3", m.justPaired)
        m = CoreState.reduce(m, CoreEvent.Computers(listOf(computer("1"), computer("2"), computer("3"))))
        assertEquals(listOf("1", "2", "3"), m.computers.map { it.key })
        // A failure stays on screen with its reason.
        m = CoreState.reduce(m, CoreEvent.Pair(8u, PairProgress.Failed("busy")))
        assertEquals(PairProgress.Failed("busy"), m.pairing?.step)
    }

    @Test fun legacy_and_core_merge_by_remote_id() {
        val web = listOf(
            Computer("w1", "Desk", "desk", "https://relay.example", "111-222-333", "relay"),
            Computer("w2", "Old laptop", "", "https://relay.example", "444555666", "relay"),
        )
        var m = CoreState.legacy(HomeModel(), web)
        m = CoreState.reduce(m, CoreEvent.Computers(listOf(computer("111222333"))))
        // One card for the computer both know, its web key kept for the doors; the other asks to pair.
        assertEquals(2, m.computers.size)
        val both = m.computers.first { it.remoteId == "111222333" }
        assertFalse(both.needsAppPairing)
        assertEquals("w1", both.webKey)
        val webOnly = m.computers.first { it.needsAppPairing }
        assertEquals("Old laptop", webOnly.name)
        assertEquals("w2", webOnly.webKey)
        // The web part forgets one: its card goes; nothing is duplicated either way.
        m = CoreState.legacy(m, web.take(1))
        assertEquals(listOf("111222333"), m.computers.map { it.key })
    }

    @Test fun sessions_fill_running_naming_the_computer() {
        val m = CoreState.reduce(two(), CoreEvent.Sessions("2", listOf(SessionInfo("1:1", "firefox", 2u, "Pixel", 1u))))
        assertEquals(1, m.computers[1].sessions)
        assertEquals("box-2", m.running.single().computerName)
        val none = CoreState.reduce(m, CoreEvent.Sessions("2", emptyList()))
        assertTrue(none.running.isEmpty())
    }
}
