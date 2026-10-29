package io.github.sandptel.wado.bridge

import org.json.JSONArray
import org.json.JSONObject

/** A computer this phone has added, as the client's settings name it (js/native_sync.js). */
data class Computer(
    val key: String,
    val name: String,
    val host: String,
    val relayUrl: String,
    val remoteId: String,
    val connMode: String,
) {
    /** What a card calls it: the person's name for it, else its hostname, else its Remote ID. */
    val title: String
        get() = name.ifBlank { host.ifBlank { if (remoteId.isNotBlank()) "Computer $remoteId" else "A computer" } }

    val relayed get() = connMode == "relay" && relayUrl.isNotBlank() && remoteId.isNotBlank()
}

/** The computers and which is selected (the last used one). */
data class Computers(val list: List<Computer> = emptyList(), val selected: Int = 0) {
    val current: Computer? get() = list.getOrNull(selected) ?: list.firstOrNull()

    fun toJson(): JSONObject = JSONObject().put("selected", selected).put("profiles", JSONArray().apply {
        list.forEach {
            put(JSONObject().put("key", it.key).put("name", it.name).put("host", it.host)
                .put("relay_url", it.relayUrl).put("remote_id", it.remoteId).put("conn_mode", it.connMode))
        }
    })

    companion object {
        fun parse(o: JSONObject?): Computers {
            if (o == null) return Computers()
            val arr = o.optJSONArray("profiles") ?: JSONArray()
            val list = (0 until arr.length()).mapNotNull { i ->
                arr.optJSONObject(i)?.let { p ->
                    val relay = p.optString("relay_url")
                    val id = p.optString("remote_id")
                    Computer(
                        key = p.optString("key").ifEmpty { "$relay|$id" },
                        name = p.optString("name"),
                        host = p.optString("host"),
                        relayUrl = relay,
                        remoteId = id,
                        connMode = p.optString("conn_mode"),
                    )
                }
            }
            return Computers(list, o.optInt("selected", 0).coerceIn(0, (list.size - 1).coerceAtLeast(0)))
        }
    }
}

/** What the computer is playing (its MPRIS player) and its output volume. */
data class NowPlaying(
    val bus: String = "",
    val app: String = "",
    val title: String = "",
    val artist: String = "",
    val art: String = "",
    val playing: Boolean = false,
    val positionMs: Long = 0,
    val lengthMs: Long = 0,
    val canNext: Boolean = false,
    val canPrev: Boolean = false,
    val canSeek: Boolean = false,
    /** 0..1, or -1 when the computer did not say. */
    val volume: Float = -1f,
    val muted: Boolean = false,
    val sink: String = "",
) {
    val isEmpty get() = bus.isEmpty()

    companion object {
        fun parse(o: JSONObject): NowPlaying = NowPlaying(
            bus = o.optString("bus"),
            app = o.optString("app"),
            title = o.optString("title"),
            artist = o.optString("artist"),
            art = o.optString("art"),
            playing = o.optBoolean("playing"),
            positionMs = o.optLong("position_ms"),
            lengthMs = o.optLong("length_ms"),
            canNext = o.optBoolean("can_next"),
            canPrev = o.optBoolean("can_prev"),
            canSeek = o.optBoolean("can_seek"),
            volume = o.optDouble("volume", -1.0).toFloat(),
            muted = o.optBoolean("muted"),
            sink = o.optString("sink"),
        )
    }
}
