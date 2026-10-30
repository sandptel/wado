package io.github.sandptel.wado.core

import io.github.sandptel.wado.bridge.Computer
import uniffi.wado_mobile_core.ComputerInfo
import uniffi.wado_mobile_core.CoreEvent
import uniffi.wado_mobile_core.LinkInfo
import uniffi.wado_mobile_core.PairProgress

/**
 * The home model from the core's events, as a pure reducer (unit-tested on the JVM). Cards are the
 * core's computers, then the ones only the web part knows ("Pair the app"); one computer is never
 * two cards — they are matched by Remote ID.
 */
object CoreState {
    private fun digits(s: String) = s.filter(Char::isDigit)

    private fun link(l: LinkInfo) = LinkModel(l.kind, l.words, l.tone, l.tech, l.caps)

    /** Cards for `core` computers, keeping what earlier events said about each, plus legacy. */
    private fun cards(core: List<ComputerInfo>, old: List<CardModel>, legacy: List<Computer>): List<CardModel> {
        val byKey = old.associateBy { it.key }
        val mine = core.map { c ->
            val was = byKey[c.key]
            val web = legacy.firstOrNull { digits(it.remoteId) == digits(c.remoteId) && digits(c.remoteId).isNotEmpty() }
            CardModel(
                key = c.key,
                name = c.name,
                remoteId = c.remoteId,
                relayUrl = c.relayUrl,
                osLine = was?.osLine?.takeIf { it.isNotEmpty() } ?: listOf(c.os, c.desktop).filter { it.isNotEmpty() }.joinToString(" · "),
                logo = c.logo,
                hue = c.hue.toInt(),
                link = was?.link ?: LinkModel(),
                sessions = was?.sessions ?: 0,
                playing = was?.playing ?: false,
                webKey = web?.key,
            )
        }
        val known = core.map { digits(it.remoteId) }.toSet()
        val webOnly = legacy.filter { digits(it.remoteId) !in known }.map { w ->
            CardModel(
                key = "web:" + w.key,
                name = w.title,
                remoteId = w.remoteId,
                relayUrl = w.relayUrl,
                needsAppPairing = true,
                webKey = w.key,
            )
        }
        return mine + webOnly
    }

    /** The web part's computers changed. */
    fun legacy(m: HomeModel, legacy: List<Computer>): HomeModel {
        val core = m.computers.filter { !it.needsAppPairing }.map {
            ComputerInfo(it.key, it.name, it.remoteId, it.relayUrl, "", it.logo, "", it.hue.toUByte())
        }
        return m.copy(legacy = legacy, computers = cards(core, m.computers, legacy))
    }

    fun reduce(m: HomeModel, ev: CoreEvent): HomeModel = when (ev) {
        is CoreEvent.Computers -> m.copy(
            computers = cards(ev.list, m.computers, m.legacy),
            running = m.running.filter { r -> ev.list.any { it.key == r.computerKey } },
        )
        is CoreEvent.Link -> m.update(ev.key) { it.copy(link = link(ev.link)) }
        is CoreEvent.Host -> m.update(ev.key) { c ->
            c.copy(
                logo = ev.host.logo,
                osLine = listOf(ev.host.os, ev.host.desktop).filter { it.isNotEmpty() }.joinToString(" · "),
                playing = ev.host.playing?.playing == true,
            )
        }
        is CoreEvent.Sessions -> {
            val name = m.computers.firstOrNull { it.key == ev.key }?.name.orEmpty()
            val rows = ev.rows.map {
                RunningModel(ev.key, name, it.instance, it.appsFirst, it.appsMore.toInt(), it.lead, it.participants.toInt())
            }
            m.update(ev.key) { it.copy(sessions = ev.rows.size) }
                .copy(running = m.running.filter { it.computerKey != ev.key } + rows)
        }
        is CoreEvent.Pair -> when (val s = ev.step) {
            is PairProgress.Done -> m.copy(pairing = null, justPaired = s.key)
            else -> m.copy(pairing = PairModel(ev.op.toLong(), s), justPaired = null)
        }
        is CoreEvent.Answer -> m
    }

    private fun HomeModel.update(key: String, f: (CardModel) -> CardModel) =
        copy(computers = computers.map { if (it.key == key) f(it) else it })
}
