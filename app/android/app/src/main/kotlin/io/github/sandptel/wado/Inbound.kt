package io.github.sandptel.wado

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import androidx.core.content.IntentCompat

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
}
