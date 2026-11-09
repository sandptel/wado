package io.github.sandptel.wado

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import androidx.core.content.IntentCompat
import androidx.core.content.pm.ShortcutManagerCompat

/**
 * What opened the app, for Dart (`wado/os` → `intent`): a link (`wado://pair?…`, `wado://go/<door>`,
 * the QR's https link) or a share ("Send to computer": files by content uri, text). Files are read
 * only when the app sends them (`read`).
 */
object Inbound {
    fun of(intent: Intent?, resolver: ContentResolver): Map<String, Any>? {
        if (intent == null) return null
        if (intent.action == Intent.ACTION_SEND || intent.action == Intent.ACTION_SEND_MULTIPLE) {
            val uris = buildList {
                if (intent.action == Intent.ACTION_SEND_MULTIPLE) {
                    IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)?.let(::addAll)
                } else {
                    IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)?.let(::add)
                }
                intent.clipData?.let { c -> for (i in 0 until c.itemCount) c.getItemAt(i).uri?.let { if (it !in this) add(it) } }
            }
            return mapOf(
                "kind" to "share",
                "text" to (intent.getStringExtra(Intent.EXTRA_TEXT) ?: ""),
                "files" to uris.map { describe(it, resolver, intent.type) },
                // Which of the share sheet's wado entries: ShareSend · ShareOpen · ShareCopy.
                "via" to (intent.component?.className?.substringAfterLast('.') ?: ""),
                // A computer picked in the sheet's top row (Shortcut.shareTargets): its key.
                "computer" to (intent.getStringExtra(ShortcutManagerCompat.EXTRA_SHORTCUT_ID)?.removePrefix(Shortcut.TO) ?: ""),
            )
        }
        val data = intent.dataString ?: return null
        return mapOf("kind" to "link", "url" to data)
    }

    private fun describe(uri: Uri, resolver: ContentResolver, fallback: String?): Map<String, Any> {
        var name = uri.lastPathSegment?.substringAfterLast('/') ?: "shared"
        var size = -1L
        runCatching {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { c ->
                if (c.moveToFirst()) {
                    c.getString(0)?.let { name = it }
                    if (!c.isNull(1)) size = c.getLong(1)
                }
            }
        }
        val mime = resolver.getType(uri) ?: fallback?.takeIf { !it.contains('*') } ?: ""
        return mapOf("uri" to uri.toString(), "name" to name, "mime" to mime, "size" to size)
    }

    fun read(uri: String, resolver: ContentResolver): ByteArray? =
        runCatching { resolver.openInputStream(Uri.parse(uri))?.use { it.readBytes() } }.getOrNull()

    /** Shared files read a piece at a time (`readOpen` / `readNext` / `readClose`): a long video
     * never sits in the app's memory whole. */
    private val open = java.util.concurrent.ConcurrentHashMap<Int, java.io.InputStream>()
    private val next = java.util.concurrent.atomic.AtomicInteger(1)

    /** (id, size): the size from the file itself when the sharing app did not say. */
    fun readOpen(uri: String, resolver: ContentResolver): Map<String, Any>? = runCatching {
        val u = Uri.parse(uri)
        val size = runCatching { resolver.openAssetFileDescriptor(u, "r")?.use { it.length } }.getOrNull() ?: -1L
        val s = resolver.openInputStream(u) ?: return null
        val id = next.getAndIncrement()
        open[id] = s
        mapOf("id" to id, "size" to size)
    }.getOrNull()

    /** Up to `max` bytes; empty at the end. */
    fun readNext(id: Int, max: Int): ByteArray {
        val s = open[id] ?: return ByteArray(0)
        val buf = ByteArray(max)
        var got = 0
        while (got < max) {
            val n = s.read(buf, got, max - got)
            if (n < 0) break
            got += n
        }
        return if (got == max) buf else buf.copyOf(got)
    }

    fun readClose(id: Int) {
        open.remove(id)?.close()
    }
}
