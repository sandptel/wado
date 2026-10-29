package io.github.sandptel.wado.share

/**
 * What a share sheet handed over, in the shape "Send to computer" acts on: files to upload,
 * links to open on the computer, and plain text for its clipboard.
 *
 * The Intent reading lives in [ShareIntent]; this is the pure part — splitting shared text into
 * the links it holds and what is left — so it is unit-tested.
 */
data class SharePayload(
    val files: List<ShareStore.Item> = emptyList(),
    val links: List<String> = emptyList(),
    val text: String = "",
) {
    val isEmpty get() = files.isEmpty() && links.isEmpty() && text.isBlank()

    /** "2 photos", "a link", "some text" — the share picker's headline. */
    fun describe(): String {
        val parts = mutableListOf<String>()
        if (files.isNotEmpty()) {
            val images = files.all { it.mime.startsWith("image/") }
            val videos = files.all { it.mime.startsWith("video/") }
            val noun = when {
                images -> "photo"
                videos -> "video"
                else -> "file"
            }
            parts += if (files.size == 1) "a $noun" else "${files.size} ${noun}s"
        }
        if (links.isNotEmpty()) parts += if (links.size == 1) "a link" else "${links.size} links"
        if (parts.isEmpty() && text.isNotBlank()) parts += "some text"
        return parts.joinToString(" and ")
    }
}

object ShareParser {
    private val LINK = Regex("""https?://[^\s<>"']+""", RegexOption.IGNORE_CASE)

    /** Shared text → its http(s) links (trailing punctuation dropped) and the text itself. */
    fun fromText(text: String?): SharePayload {
        val t = text.orEmpty().trim()
        if (t.isEmpty()) return SharePayload()
        val links = LINK.findAll(t).map { it.value.trimEnd('.', ',', ')', ';', '!', '?') }.distinct().toList()
        return SharePayload(links = links, text = t)
    }
}
