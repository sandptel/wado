package io.github.sandptel.wado.links

import java.net.URI
import java.net.URLDecoder
import java.net.URLEncoder

/**
 * The links the app follows into the client, as pure functions (unit-tested):
 *
 * - **Connect links** — what a computer's QR holds (`wado qr`, the TUI's pair page):
 *   `<client url>?relay=…&id=…&pair=…&hk=…`, or the same as `wado://pair?…`. Only those four
 *   parameters are carried over, onto the app's own origin, where js/storage.js takes them in.
 * - **Doors** — the app-icon shortcuts' `go` (`trackpad`, `files`, `shell`, `session`) and the
 *   Virtual input links (`keyboard`, `gamepad`, `input`, opened by the web client), the
 *   same set and meaning as the web app's manifest shortcuts (crates/client/src/deeplink.rs).
 */
object DeepLinks {
    val DOORS = listOf("trackpad", "files", "shell", "session", "keyboard", "gamepad", "input")
    private val CONNECT_KEYS = listOf("relay", "id", "pair", "hk")

    /** A door name, normalised, or null if it is not one. */
    fun door(go: String?): String? = go?.trim()?.lowercase()?.let { g ->
        when (g) {
            "desktop" -> "session"
            in DOORS -> g
            else -> null
        }
    }

    /** A shortcut's door link, `wado://go/<door>[?key=<profile key>]`: (door, key or ""). */
    data class Door(val door: String, val key: String)

    fun door(uri: URI): Door? {
        if (uri.scheme?.lowercase() != "wado" || uri.host?.lowercase() != "go") return null
        val d = door(uri.path?.trim('/')) ?: return null
        return Door(d, uri.rawQuery?.let { query(it)["key"] }.orEmpty())
    }

    fun doorLink(door: String, key: String = ""): String =
        "wado://go/$door" + if (key.isEmpty()) "" else "?key=${enc(key)}"

    /** The parameters of a connect link, or null if `text` is not one (it must name an `id`). */
    fun connect(text: String?): Map<String, String>? {
        val raw = text?.trim().orEmpty()
        if (raw.isEmpty()) return null
        val uri = try {
            URI(raw)
        } catch (_: Exception) {
            return null
        }
        if (uri.scheme?.lowercase() !in setOf("https", "http", "wado")) return null
        val params = query(uri.rawQuery ?: return null)
        val picked = CONNECT_KEYS.mapNotNull { k -> params[k]?.takeIf { it.isNotBlank() }?.let { k to it } }.toMap()
        val id = picked["id"]?.filter(Char::isDigit).orEmpty()
        if (id.isEmpty()) return null
        val relay = picked["relay"]
        if (relay != null && !relay.startsWith("https://") && !relay.startsWith("http://")) return null
        return picked + ("id" to id)
    }

    /** `?relay=…&id=…` for the app's origin. */
    fun connectQuery(params: Map<String, String>): String =
        "?" + CONNECT_KEYS.mapNotNull { k -> params[k]?.let { "$k=${enc(it)}" } }.joinToString("&")

    /** True when this connect link carries a pairing code (a first pairing, not a re-link). */
    fun pairs(params: Map<String, String>) = !params["pair"].isNullOrBlank()

    private fun query(raw: String): Map<String, String> = raw.split('&').mapNotNull { kv ->
        val i = kv.indexOf('=')
        if (i <= 0) null else dec(kv.substring(0, i)) to dec(kv.substring(i + 1))
    }.toMap()

    private fun dec(s: String) = URLDecoder.decode(s, "UTF-8")
    private fun enc(s: String) = URLEncoder.encode(s, "UTF-8")
}
