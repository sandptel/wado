package io.github.sandptel.wado.core

import io.github.sandptel.wado.bridge.Computer
import uniffi.wado_mobile_core.LinkTone
import uniffi.wado_mobile_core.PairProgress

/** A card's link: the core's plain words and how they are drawn. */
data class LinkModel(
    val kind: String = "idle",
    val words: String = "Not connected",
    val tone: LinkTone = LinkTone.STILL,
    val tech: String = "Idle",
    val caps: List<String> = emptyList(),
) {
    val online get() = kind == "open"
}

/** One card on home: a computer the core has paired, or one only the web part knows. */
data class CardModel(
    val key: String,
    val name: String,
    val remoteId: String,
    val relayUrl: String,
    /** "NixOS 26.11 · Hyprland", or what the core cached while it was offline. */
    val osLine: String = "",
    val logo: String = "linux",
    /** Base16 accent slot 0..7 (base08..base0F). */
    val hue: Int = 0,
    val link: LinkModel = LinkModel(),
    val sessions: Int = 0,
    val playing: Boolean = false,
    /** Known to the web part only: the app's core is a new device and pairs once (A9.5). */
    val needsAppPairing: Boolean = false,
    /** The web part's key for this computer, when it knows it — what its doors open. */
    val webKey: String? = null,
)

/** A running session, naming its computer. */
data class RunningModel(
    val computerKey: String,
    val computerName: String,
    val instance: String,
    val appsFirst: String,
    val appsMore: Int,
    val lead: String,
    val participants: Int,
)

/** A pairing ceremony in progress (or just failed). */
data class PairModel(val op: Long, val step: PairProgress)

data class HomeModel(
    val computers: List<CardModel> = emptyList(),
    val running: List<RunningModel> = emptyList(),
    val pairing: PairModel? = null,
    /** The key a pairing just added: home scrolls to it, the pair screen closes. */
    val justPaired: String? = null,
    /** What the web part last reported (Prefs.computers): merged by Remote ID. */
    val legacy: List<Computer> = emptyList(),
)
