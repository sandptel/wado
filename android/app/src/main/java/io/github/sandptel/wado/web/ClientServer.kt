package io.github.sandptel.wado.web

import android.content.Context
import android.net.Uri
import android.webkit.WebResourceResponse
import androidx.webkit.WebViewAssetLoader
import io.github.sandptel.wado.share.ShareStore
import java.io.IOException

/**
 * Serves the bundled client, and the files a share hands over, on the app's own secure origin.
 *
 *   https://appassets.androidplatform.net/            → assets/client/… (a route → index.html)
 *   https://appassets.androidplatform.net/share/t/n   → the n-th item of share t (ShareStore)
 *
 * Nothing else is served: every other host goes to the network as usual.
 */
class ClientServer(private val context: Context) {
    val loader: WebViewAssetLoader = WebViewAssetLoader.Builder()
        .setDomain(DOMAIN)
        .addPathHandler("/share/", ::share)
        .addPathHandler("/", ::client)
        .build()

    fun intercept(url: Uri): WebResourceResponse? = loader.shouldInterceptRequest(url)

    private fun client(path: String): WebResourceResponse? {
        val asset = AssetPaths.resolve(path) ?: return notFound()
        val opened = open(asset)
            ?: if (AssetPaths.isRoute(path)) open("${AssetPaths.ROOT}/${AssetPaths.INDEX}") else null
        val (name, stream) = opened ?: return notFound()
        val mime = AssetPaths.mimeType(name)
        return WebResourceResponse(mime, AssetPaths.encoding(mime), stream).apply {
            // The client is fixed for this APK: it changes only with an app update.
            responseHeaders = mapOf("Cache-Control" to "no-cache")
        }
    }

    private fun open(asset: String) = try {
        asset to context.assets.open(asset)
    } catch (_: IOException) {
        null
    }

    private fun share(path: String): WebResourceResponse? {
        val item = ShareStore.item(path) ?: return notFound()
        val stream = try {
            context.contentResolver.openInputStream(item.uri)
        } catch (_: Exception) {
            null
        } ?: return notFound()
        return WebResourceResponse(item.mime.ifEmpty { "application/octet-stream" }, null, stream)
    }

    private fun notFound() =
        WebResourceResponse("text/plain", "utf-8", 404, "Not Found", emptyMap(), "".byteInputStream())

    companion object {
        const val DOMAIN = WebViewAssetLoader.DEFAULT_DOMAIN
        const val ORIGIN = "https://$DOMAIN"
        const val HOME = "$ORIGIN/"
    }
}
