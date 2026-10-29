package io.github.sandptel.wado.share

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import androidx.core.content.IntentCompat

/** Reads an ACTION_SEND / ACTION_SEND_MULTIPLE intent into a [SharePayload]. */
object ShareIntent {
    fun isShare(intent: Intent?) =
        intent?.action == Intent.ACTION_SEND || intent?.action == Intent.ACTION_SEND_MULTIPLE

    fun read(intent: Intent, resolver: ContentResolver): SharePayload {
        val uris = buildList {
            if (intent.action == Intent.ACTION_SEND_MULTIPLE) {
                IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)
                    ?.let(::addAll)
            } else {
                IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)?.let(::add)
            }
            intent.clipData?.let { clip ->
                for (i in 0 until clip.itemCount) clip.getItemAt(i).uri?.let { if (it !in this) add(it) }
            }
        }
        val files = uris.map { describe(it, resolver, intent.type) }
        val text = ShareParser.fromText(intent.getStringExtra(Intent.EXTRA_TEXT))
        return text.copy(files = files)
    }

    private fun describe(uri: Uri, resolver: ContentResolver, fallbackMime: String?): ShareStore.Item {
        var name = uri.lastPathSegment?.substringAfterLast('/') ?: "shared"
        var size = -1L
        try {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)
                ?.use { c ->
                    if (c.moveToFirst()) {
                        c.getString(0)?.let { name = it }
                        if (!c.isNull(1)) size = c.getLong(1)
                    }
                }
        } catch (_: Exception) {
        }
        val mime = resolver.getType(uri) ?: fallbackMime?.takeIf { !it.contains('*') } ?: ""
        return ShareStore.Item(uri, name, mime, size)
    }
}
