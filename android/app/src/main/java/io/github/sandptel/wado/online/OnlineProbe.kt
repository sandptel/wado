package io.github.sandptel.wado.online

import io.github.sandptel.wado.bridge.Computer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/**
 * Is a computer's daemon at its relay right now — the relay's public `GET /online/:id`, the same
 * probe the web client's home runs (js/online.js). Says nothing about pairing or grants; that
 * is the page's to know once it dials.
 */
object OnlineProbe {
    enum class Seen { ONLINE, OFFLINE, UNREACHABLE, UNKNOWN }

    fun url(c: Computer): String? =
        if (c.relayed) c.relayUrl.trimEnd('/') + "/online/" + java.net.URLEncoder.encode(c.remoteId, "UTF-8") else null

    /** `{"daemons": n}` → online when n > 0. */
    fun parse(body: String): Seen = runCatching {
        if (JSONObject(body).optInt("daemons", 0) > 0) Seen.ONLINE else Seen.OFFLINE
    }.getOrDefault(Seen.UNREACHABLE)

    suspend fun probe(c: Computer): Seen = withContext(Dispatchers.IO) {
        val u = url(c) ?: return@withContext Seen.UNKNOWN
        runCatching {
            val conn = URL(u).openConnection() as HttpURLConnection
            conn.connectTimeout = 6000
            conn.readTimeout = 6000
            conn.useCaches = false
            try {
                parse(conn.inputStream.bufferedReader().readText())
            } finally {
                conn.disconnect()
            }
        }.getOrDefault(Seen.UNREACHABLE)
    }

    suspend fun probeAll(list: List<Computer>): Map<String, Seen> = coroutineScope {
        list.map { c -> async { c.key to probe(c) } }.awaitAll().toMap()
    }
}
