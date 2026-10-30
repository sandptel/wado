package io.github.sandptel.wado.ui.theme

import android.content.Context
import org.json.JSONArray

/**
 * The base16 schemes the web client offers (`crates/client/src/schemes.json`, copied into the
 * APK's assets at build time), by name.
 */
object Schemes {
    @Volatile private var all: List<Pair<String, Base16>>? = null

    fun parse(json: String): List<Pair<String, Base16>> {
        val a = JSONArray(json)
        return (0 until a.length()).mapNotNull { i ->
            val e = a.getJSONArray(i)
            val hexes = e.getJSONArray(1).let { h -> (0 until h.length()).map(h::getString) }
            Base16.parse(hexes)?.let { e.getString(0) to it }
        }
    }

    fun list(context: Context): List<Pair<String, Base16>> = all ?: runCatching {
        parse(context.assets.open("schemes.json").bufferedReader().use { it.readText() })
    }.getOrDefault(emptyList()).also { all = it }

    fun byName(context: Context, name: String): Base16? = list(context).firstOrNull { it.first == name }?.second

    /** `s` with the accent role (base0D) taken from slot 8 + `slot`; `s` itself for -1. */
    fun withAccent(s: Base16, slot: Int): Base16 {
        if (slot !in 0..7) return s
        val slots = s.slots.toMutableList()
        slots[0xD] = s.slots[0x8 + slot]
        return Base16(slots)
    }
}
