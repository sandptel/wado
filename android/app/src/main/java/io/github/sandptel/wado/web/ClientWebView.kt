package io.github.sandptel.wado.web

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Message
import android.view.View
import android.webkit.ConsoleMessage
import android.webkit.PermissionRequest
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import io.github.sandptel.wado.BuildConfig
import io.github.sandptel.wado.bridge.BridgeProtocol
import io.github.sandptel.wado.bridge.Hub

/**
 * The Android System WebView (Chromium, so WebRTC works — unlike WebKitGTK) hosting the
 * bundled client on its own secure origin, with the `wadoNative` bridge injected for that origin
 * alone. One per process (WadoApp.web), outliving any activity: it holds the relay link.
 */
class ClientWebView(context: Context) {
    /** The activity showing this view now; null while none is (the page runs on, kept by
     *  "Stay connected"), when every ask is refused. */
    var host: Host? = null

    /** What only the activity can do for the page. */
    interface Host {
        /** The page wants the camera / microphone; call back with what was granted. */
        fun askMedia(camera: Boolean, mic: Boolean, done: (camera: Boolean, mic: Boolean) -> Unit)
        /** `<input type=file>`: pick files, call back with their URIs (null: cancelled). */
        fun pickFiles(multiple: Boolean, mimeTypes: Array<String>, done: (Array<Uri>?) -> Unit)
        /** A link off this origin: open it outside the app. */
        fun openExternal(uri: Uri)
        /** Element fullscreen (a video): show `view` over everything, or take it down (null). */
        fun fullscreen(view: View?, exit: (() -> Unit)?)
    }

    private val server = ClientServer(context)
    val view: WebView = WebView(context)

    init {
        configure()
    }

    @SuppressLint("SetJavaScriptEnabled", "RequiresFeature")
    private fun configure() {
        WebView.setWebContentsDebuggingEnabled(BuildConfig.DEBUG)
        view.setBackgroundColor(Color.BLACK)
        view.overScrollMode = View.OVER_SCROLL_NEVER
        view.isFocusable = true
        view.isFocusableInTouchMode = true
        with(view.settings) {
            javaScriptEnabled = true
            domStorageEnabled = true
            mediaPlaybackRequiresUserGesture = false
            allowFileAccess = false
            allowContentAccess = false
            setSupportMultipleWindows(false)
            setSupportZoom(false)
            builtInZoomControls = false
            displayZoomControls = false
            useWideViewPort = true
            loadWithOverviewMode = false
            userAgentString = "$userAgentString wado-android/${BuildConfig.VERSION_NAME}"
        }
        // Passkeys: no Digital Asset Links file can vouch for this origin, so the system's passkey
        // providers would refuse it. The page's navigator.credentials is answered by the app's own
        // Keystore authenticator instead (js/native_passkey.js, passkey/Authenticator.kt).
        val origins = setOf(ClientServer.ORIGIN)
        if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            WebViewCompat.addDocumentStartJavaScript(
                view,
                BridgeProtocol.infoScript(
                    BuildConfig.VERSION_NAME,
                    Build.VERSION.SDK_INT,
                    // As the Notification API names it (js/push.js); read when the page loads.
                    if (androidx.core.app.NotificationManagerCompat.from(view.context).areNotificationsEnabled()) "granted" else "denied",
                ),
                origins,
            )
        }
        if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            WebViewCompat.addWebMessageListener(view, BridgeProtocol.OBJECT, origins) { _, message, origin, isMainFrame, reply ->
                if (isMainFrame && origin.toString().trimEnd('/') == ClientServer.ORIGIN) {
                    Hub.receive(message.data, reply)
                }
            }
        }
        view.webViewClient = Client()
        view.webChromeClient = Chrome()
    }

    fun loadHome(query: String = "") = view.loadUrl(ClientServer.HOME + query)

    private inner class Client : WebViewClient() {
        override fun shouldInterceptRequest(v: WebView, request: WebResourceRequest): WebResourceResponse? =
            server.intercept(request.url)

        override fun shouldOverrideUrlLoading(v: WebView, request: WebResourceRequest): Boolean {
            val url = request.url
            if (url.scheme == "https" && url.host == ClientServer.DOMAIN) return false
            host?.openExternal(url)
            return true
        }

        override fun onPageStarted(v: WebView, url: String?, favicon: android.graphics.Bitmap?) {
            Hub.pageGone()
        }
    }

    private inner class Chrome : WebChromeClient() {
        override fun onPermissionRequest(request: PermissionRequest) {
            if (request.origin.host != ClientServer.DOMAIN) return request.deny()
            val wantsCamera = PermissionRequest.RESOURCE_VIDEO_CAPTURE in request.resources
            val wantsMic = PermissionRequest.RESOURCE_AUDIO_CAPTURE in request.resources
            if (!wantsCamera && !wantsMic) return request.deny()
            val h = host ?: return request.deny()
            h.askMedia(wantsCamera, wantsMic) { cam, mic ->
                val grant = buildList {
                    if (wantsCamera && cam) add(PermissionRequest.RESOURCE_VIDEO_CAPTURE)
                    if (wantsMic && mic) add(PermissionRequest.RESOURCE_AUDIO_CAPTURE)
                }
                if (grant.isEmpty()) request.deny() else request.grant(grant.toTypedArray())
            }
        }

        override fun onShowFileChooser(
            v: WebView,
            callback: ValueCallback<Array<Uri>>,
            params: FileChooserParams,
        ): Boolean {
            val multiple = params.mode == FileChooserParams.MODE_OPEN_MULTIPLE
            val types = params.acceptTypes.filter { it.isNotBlank() && it.contains('/') }.toTypedArray()
            val h = host ?: return false
            h.pickFiles(multiple, types) { callback.onReceiveValue(it) }
            return true
        }

        override fun onShowCustomView(view: View, callback: CustomViewCallback) {
            val h = host ?: return callback.onCustomViewHidden()
            h.fullscreen(view) { callback.onCustomViewHidden() }
        }

        override fun onHideCustomView() {
            host?.fullscreen(null, null)
        }

        override fun onCreateWindow(v: WebView, dialog: Boolean, user: Boolean, resultMsg: Message?) = false

        override fun onConsoleMessage(m: ConsoleMessage): Boolean {
            if (BuildConfig.DEBUG) android.util.Log.d("wado-page", "${m.messageLevel()}: ${m.message()}")
            return true
        }
    }
}
