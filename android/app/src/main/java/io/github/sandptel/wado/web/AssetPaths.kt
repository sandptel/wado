package io.github.sandptel.wado.web

/**
 * Where a request for the bundled web client is found in the APK, and what it is.
 *
 * The client is `dx build` output copied to `assets/client/` (scripts/android-build.sh) and served
 * at `https://appassets.androidplatform.net/` — a secure context, so WebRTC, WebCrypto and
 * storage behave as on the real site. Pure, so the mapping is unit-tested.
 */
object AssetPaths {
    const val ROOT = "client"
    const val INDEX = "index.html"

    /**
     * The asset path for a request path (no leading slash, already stripped of the handler's
     * prefix). `null` for anything that tries to leave the client directory.
     */
    fun resolve(path: String): String? {
        val clean = path.substringBefore('?').substringBefore('#').trimStart('/')
        if (clean.isEmpty()) return "$ROOT/$INDEX"
        val parts = clean.split('/')
        if (parts.any { it == ".." || it == "." || it.isEmpty() }) return null
        return "$ROOT/$clean"
    }

    /** The page itself, for a path that names no file (a client route). */
    fun isRoute(path: String): Boolean {
        val last = path.substringBefore('?').trimEnd('/').substringAfterLast('/')
        return last.isEmpty() || !last.contains('.')
    }

    private val TYPES = mapOf(
        "html" to "text/html",
        "js" to "text/javascript",
        "mjs" to "text/javascript",
        "css" to "text/css",
        "wasm" to "application/wasm",
        "json" to "application/json",
        "webmanifest" to "application/manifest+json",
        "svg" to "image/svg+xml",
        "png" to "image/png",
        "jpg" to "image/jpeg",
        "jpeg" to "image/jpeg",
        "webp" to "image/webp",
        "ico" to "image/x-icon",
        "woff2" to "font/woff2",
        "woff" to "font/woff",
        "ttf" to "font/ttf",
        "txt" to "text/plain",
        "map" to "application/json",
    )

    /** The MIME type by extension. WebAssembly must be `application/wasm` to stream-compile. */
    fun mimeType(path: String): String =
        TYPES[path.substringAfterLast('.', "").lowercase()] ?: "application/octet-stream"

    /** Text types are served as UTF-8. */
    fun encoding(mime: String): String? =
        if (mime.startsWith("text/") || mime.endsWith("json") || mime.endsWith("xml")) "utf-8" else null
}
