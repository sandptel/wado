package io.github.sandptel.wado.share

import android.net.Uri
import java.security.SecureRandom

/**
 * The files of a share, held for the page to fetch from `/share/<token>/<n>` (ClientServer).
 * A token is random and single-share; nothing is copied, the page reads straight from the
 * other app's content URI while this app still holds the read grant the share came with.
 */
object ShareStore {
    data class Item(val uri: Uri, val name: String, val mime: String, val size: Long)

    private val shares = LinkedHashMap<String, List<Item>>()
    private val random = SecureRandom()

    /** Keep these items; returns the token they are served under. Only the last few are kept. */
    @Synchronized
    fun put(items: List<Item>): String {
        val token = ByteArray(12).also(random::nextBytes).joinToString("") { "%02x".format(it) }
        shares[token] = items
        while (shares.size > 4) shares.remove(shares.keys.first())
        return token
    }

    /** `path` is what follows `/share/`: `<token>/<n>`. */
    @Synchronized
    fun item(path: String): Item? {
        val (token, n) = path.split('/').let { if (it.size == 2) it[0] to it[1] else return null }
        return shares[token]?.getOrNull(n.toIntOrNull() ?: return null)
    }

    fun url(origin: String, token: String, n: Int) = "$origin/share/$token/$n"
}
