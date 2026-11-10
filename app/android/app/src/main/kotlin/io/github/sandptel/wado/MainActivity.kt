package io.github.sandptel.wado

import android.Manifest
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.codescanner.GmsBarcodeScannerOptions
import com.google.mlkit.vision.codescanner.GmsBarcodeScanning
import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Intent
import android.os.PersistableBundle
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.content.pm.PackageManager
import android.os.Build
import android.view.HapticFeedbackConstants
import android.view.KeyEvent
import android.view.WindowManager
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.activity.result.contract.ActivityResultContracts
import io.flutter.embedding.android.FlutterFragmentActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import io.github.sandptel.wado.media.RemoteMedia
import io.github.sandptel.wado.notify.Notifier
import io.github.sandptel.wado.passkey.AuthErrors
import io.github.sandptel.wado.push.Push
import io.github.sandptel.wado.stay.StayConnectedService
import io.github.sandptel.wado.passkey.Authenticator

/**
 * The Flutter host. A FragmentActivity because BiometricPrompt needs one. One method channel,
 * `wado/passkey`, answers the app's passkey requests with the Keystore authenticator — every
 * request answered once, as the old app's bridge did.
 */
class MainActivity : FlutterFragmentActivity() {
    private val auth by lazy { Authenticator(this) }

    // The camera for the pairing QR: asked for when the scanner opens, never at launch.
    private var cameraAnswer: ((Boolean) -> Unit)? = null
    private val cameraAsk = registerForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        cameraAnswer?.invoke(ok)
        cameraAnswer = null
    }

    // Android 13+: notifications need asking, from the page that turns them on.
    private var notifAnswer: ((Boolean) -> Unit)? = null
    private val notifAsk = registerForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        notifAnswer?.invoke(ok)
        notifAnswer = null
    }

    private var os: MethodChannel? = null
    private val notifier by lazy { Notifier(this).also { it.createChannels() } }
    private val media by lazy { RemoteMedia(this) { op, ms -> os?.invokeMethod("media", mapOf("op" to op, "ms" to ms)) } }
    /** Volume keys drive the computer's volume while wado is in front, when Dart says there is one. */
    private var volumeKeys = false

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (volumeKeys && (keyCode == KeyEvent.KEYCODE_VOLUME_UP || keyCode == KeyEvent.KEYCODE_VOLUME_DOWN)) {
            os?.invokeMethod("volume", if (keyCode == KeyEvent.KEYCODE_VOLUME_UP) 0.05 else -0.05)
            return true
        }
        return super.onKeyDown(keyCode, event)
    }

    /**
     * The screen's fastest mode at its current size, or the phone's own choice. Some OEMs hold an
     * app that states no preference at 60 Hz on a 120 Hz panel.
     */
    @Suppress("DEPRECATION")
    private fun maxRefresh(on: Boolean) {
        val d = windowManager.defaultDisplay ?: return
        val now = d.mode
        val best = d.supportedModes
            .filter { it.physicalWidth == now.physicalWidth && it.physicalHeight == now.physicalHeight }
            .maxByOrNull { it.refreshRate }
        window.attributes = window.attributes.apply { preferredDisplayModeId = if (on && best != null) best.modeId else 0 }
    }

    /** A notification's tap or button (Notifier.ACTION), as Dart reads it. */
    private fun notifyAction(i: Intent): Map<String, String>? = if (i.action != Notifier.ACTION) null else mapOf(
        "kind" to (i.getStringExtra("kind") ?: ""), "id" to (i.getStringExtra("id") ?: ""), "action" to (i.getStringExtra("action") ?: ""),
    )

    // A new intent while running goes straight to Dart; the launching one waits until Dart asks.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        notifyAction(intent)?.let { os?.invokeMethod("notifyAction", it); return }
        Inbound.of(intent, contentResolver)?.let { os?.invokeMethod("intent", it) }
    }

    override fun configureFlutterEngine(engine: FlutterEngine) {
        super.configureFlutterEngine(engine)
        os = MethodChannel(engine.dartExecutor.binaryMessenger, "wado/os")
        os!!.setMethodCallHandler { call, result ->
            when (call.method) {
                "intent" -> {
                    result.success(notifyAction(intent)?.let { mapOf("notifyAction" to it) } ?: Inbound.of(intent, contentResolver))
                    intent = Intent() // answered once: a recreated activity does not replay it
                }
                // A file from the computer, opened in this phone's own app for its kind (a PDF in
                // the PDF app, a video in the player): the viewer's "Opens in your … app".
                "view" -> try {
                    val f = java.io.File(call.argument<String>("path")!!)
                    val uri = androidx.core.content.FileProvider.getUriForFile(this, "$packageName.files", f)
                    startActivity(Intent(Intent.ACTION_VIEW).setDataAndType(uri, call.argument<String>("mime"))
                        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION))
                    result.success(true)
                } catch (_: android.content.ActivityNotFoundException) {
                    result.success(false)
                }
                "notify" -> {
                    @Suppress("UNCHECKED_CAST")
                    notifier.show(call.argument("kind")!!, call.argument("id")!!, call.argument("title") ?: "", call.argument("body") ?: "",
                        (call.argument<List<Map<String, String>>>("actions") ?: emptyList()))
                    result.success(null)
                }
                "notifyClear" -> {
                    notifier.clear(call.argument("kind")!!, call.argument("id")!!)
                    result.success(null)
                }
                "nowPlaying" -> {
                    notifier
                    media.update(call.arguments as? Map<String, Any?> ?: emptyMap())
                    result.success(null)
                }
                "maxRefresh" -> {
                    maxRefresh(call.arguments == true)
                    result.success(null)
                }
                "stay" -> {
                    notifier
                    StayConnectedService.onStop = { runOnUiThread { os?.invokeMethod("stayOff", null) } }
                    StayConnectedService.sync(this, call.arguments == true)
                    result.success(null)
                }
                "notifications" -> if (Build.VERSION.SDK_INT < 33 ||
                    checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
                ) {
                    result.success(true)
                } else {
                    notifAnswer = { result.success(it) }
                    notifAsk.launch(Manifest.permission.POST_NOTIFICATIONS)
                }
                "pushSubscribe" -> {
                    Push.answer = { m -> runOnUiThread { os?.invokeMethod("pushEndpoint", m) } }
                    Push.subscribe(this, call.argument("vapid") ?: "", call.argument("rid") ?: "")
                    result.success(null)
                }
                "volumeKeys" -> {
                    volumeKeys = call.arguments == true
                    result.success(null)
                }
                "readOpen" -> Thread {
                    val r = Inbound.readOpen(call.argument<String>("uri")!!, contentResolver)
                    runOnUiThread { result.success(r) }
                }.start()
                "readNext" -> Thread {
                    val b = Inbound.readNext(call.argument<Int>("id")!!, call.argument<Int>("max") ?: (1 shl 20))
                    runOnUiThread { result.success(b) }
                }.start()
                "readClose" -> {
                    Inbound.readClose(call.argument<Int>("id")!!)
                    result.success(null)
                }
                "read" -> Thread {
                    val bytes = Inbound.read(call.argument<String>("uri")!!, contentResolver)
                    runOnUiThread { result.success(bytes) }
                }.start()
                // The phone's clipboard: read only while in front (Android's rule), and a copy a
                // password manager marked sensitive is said to be — clip_share never sends it.
                "clipRead" -> {
                    val cm = getSystemService(ClipboardManager::class.java)
                    val clip = runCatching { cm.primaryClip }.getOrNull()
                    val text = clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(this)?.toString()
                    val sensitive = Build.VERSION.SDK_INT >= 33 &&
                        clip?.description?.extras?.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE) == true
                    result.success(text?.let { mapOf("text" to it, "sensitive" to sensitive) })
                }
                "clipWrite" -> {
                    val cm = getSystemService(ClipboardManager::class.java)
                    val clip = ClipData.newPlainText("wado", call.argument<String>("text").orEmpty())
                    if (call.argument<Boolean>("sensitive") == true) {
                        clip.description.extras = PersistableBundle().apply { putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true) }
                    }
                    result.success(runCatching { cm.setPrimaryClip(clip) }.isSuccess)
                }
                // Answers how it played (or why not), so a phone where nothing is felt says so.
                "haptic" -> {
                    val prims = call.argument<List<Map<String, Any>>>("prims")
                    val how = try {
                        if (prims != null && compose(prims)) "composed"
                        else buzz(call.argument<String>("kind").orEmpty(), call.argument<Int>("ms") ?: 0)
                    } catch (e: Exception) {
                        "failed: ${e.javaClass.simpleName}: ${e.message}"
                    }
                    result.success(how)
                }
                "active" -> {
                    active(call.argument<Boolean>("on") == true)
                    result.success(null)
                }
                "shareTargets" -> {
                    val list = call.argument<List<Map<String, String>>>("computers").orEmpty()
                    Shortcut.shareTargets(this, list.map { (it["key"] ?: "") to (it["name"] ?: "") }.filter { it.first.isNotEmpty() })
                    result.success(null)
                }
                "shortcut" -> {
                    Shortcut.desktop(this, call.argument<String>("key").orEmpty(), call.argument<String>("title").orEmpty())
                    result.success(null)
                }
                // Google's code scanner: the QR's text, "" when put away, an error when Play
                // services cannot scan here (the Dart side then opens the in-app reader).
                "scan" -> {
                    val opts = GmsBarcodeScannerOptions.Builder().setBarcodeFormats(Barcode.FORMAT_QR_CODE).build()
                    GmsBarcodeScanning.getClient(this, opts).startScan()
                        .addOnSuccessListener { result.success(it.rawValue ?: "") }
                        .addOnCanceledListener { result.success("") }
                        .addOnFailureListener { result.error("unavailable", it.message, null) }
                }
                "camera" -> if (checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
                    result.success(true)
                } else {
                    cameraAnswer = { result.success(it) }
                    cameraAsk.launch(Manifest.permission.CAMERA)
                }
                else -> result.notImplemented()
            }
        }
        MethodChannel(engine.dartExecutor.binaryMessenger, "wado/passkey").setMethodCallHandler { call, result ->
            val id = call.argument<Int>("id") ?: 0
            val timeout = (call.argument<Int>("timeoutMs") ?: 60000).toLong()
            val answer: (Authenticator.Result) -> Unit = { r ->
                runOnUiThread {
                    when (r) {
                        is Authenticator.Result.Made -> result.success(mapOf("cred" to Authenticator.enc(r.credId), "point" to r.point))
                        is Authenticator.Result.Signed -> result.success(mapOf("cred" to Authenticator.enc(r.credId), "signature" to r.signature))
                        is Authenticator.Result.Failed -> result.success(mapOf("error" to r.why.name, "code" to r.why.code, "message" to r.why.message))
                    }
                }
            }
            when (call.method) {
                "create" -> auth.create(id, call.argument<String>("rpId")!!, timeout, answer)
                "pick" -> {
                    val allow = call.argument<List<String>>("allow").orEmpty().map(Authenticator::dec)
                    result.success(auth.pick(call.argument<String>("rpId")!!, allow)?.let(Authenticator::enc))
                }
                "get" -> auth.get(id, Authenticator.dec(call.argument<String>("cred")!!), call.argument<ByteArray>("data")!!, timeout, answer)
                "cancel" -> {
                    auth.cancel(id)
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
    }

    private val vibrator: Vibrator? by lazy {
        if (Build.VERSION.SDK_INT >= 31) getSystemService(VibratorManager::class.java)?.defaultVibrator
        else @Suppress("DEPRECATION") getSystemService(Vibrator::class.java)
    }

    /**
     * The motor itself, as the web's `navigator.vibrate` drove it: `performHapticFeedback` goes
     * quiet with the system's touch feedback off, and its ticks are too faint to feel on many
     * phones — the app's haptics went unnoticed. A length (`ms`, the gamepad's strength) is a
     * plain pulse; a kind is the phone's own predefined effect.
     */
    private fun buzz(kind: String, ms: Int): String {
        val v = vibrator
        if (v == null || !v.hasVibrator()) {
            val done = window.decorView.performHapticFeedback(haptic(kind))
            return "no vibrator; view feedback ${if (done) "played" else "refused"}"
        }
        val e = when {
            ms > 0 -> VibrationEffect.createOneShot(ms.toLong(), VibrationEffect.DEFAULT_AMPLITUDE)
            Build.VERSION.SDK_INT >= 29 -> VibrationEffect.createPredefined(
                when (kind) {
                    "tick", "toggle_off" -> VibrationEffect.EFFECT_TICK
                    "claim", "heavy" -> VibrationEffect.EFFECT_HEAVY_CLICK
                    "reject" -> VibrationEffect.EFFECT_DOUBLE_CLICK
                    else -> VibrationEffect.EFFECT_CLICK
                },
            )
            else -> VibrationEffect.createOneShot(if (kind == "tick") 6 else if (kind == "claim" || kind == "heavy") 20 else 12, VibrationEffect.DEFAULT_AMPLITUDE)
        }
        v.vibrate(e)
        return "effect"
    }

    /**
     * A touchpad feel as composition primitives (`p` name, `s` scale 0..1, `d` ms before it).
     * False when this phone cannot play every one of them: the caller falls back to a
     * predefined effect.
     */
    private fun compose(prims: List<Map<String, Any>>): Boolean {
        val v = vibrator ?: return false
        if (Build.VERSION.SDK_INT < 30 || !v.hasVibrator()) return false
        val ids = prims.map {
            when (it["p"]) {
                "click" -> VibrationEffect.Composition.PRIMITIVE_CLICK
                "tick" -> VibrationEffect.Composition.PRIMITIVE_TICK
                "low_tick" -> if (Build.VERSION.SDK_INT >= 31) VibrationEffect.Composition.PRIMITIVE_LOW_TICK else VibrationEffect.Composition.PRIMITIVE_TICK
                "thud" -> if (Build.VERSION.SDK_INT >= 31) VibrationEffect.Composition.PRIMITIVE_THUD else VibrationEffect.Composition.PRIMITIVE_CLICK
                "quick_rise" -> VibrationEffect.Composition.PRIMITIVE_QUICK_RISE
                "slow_rise" -> VibrationEffect.Composition.PRIMITIVE_SLOW_RISE
                else -> return false
            }
        }
        if (!v.areAllPrimitivesSupported(*ids.toIntArray())) return false
        val c = VibrationEffect.startComposition()
        ids.forEachIndexed { i, id ->
            val scale = (prims[i]["s"] as? Number)?.toFloat()?.coerceIn(0f, 1f) ?: 1f
            val delay = (prims[i]["d"] as? Number)?.toInt() ?: 0
            c.addPrimitive(id, scale, delay)
        }
        v.vibrate(c.compose())
        return true
    }

    /** The page's haptic kinds as the phone's own predefined effects (the old app's Haptics.kt). */
    private fun haptic(kind: String): Int {
        val sdk = Build.VERSION.SDK_INT
        return when (kind) {
            "tick" -> if (sdk >= 34) HapticFeedbackConstants.SEGMENT_FREQUENT_TICK else HapticFeedbackConstants.CLOCK_TICK
            "tap" -> HapticFeedbackConstants.VIRTUAL_KEY
            "claim" -> if (sdk >= 30) HapticFeedbackConstants.GESTURE_START else HapticFeedbackConstants.LONG_PRESS
            "confirm" -> if (sdk >= 30) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.VIRTUAL_KEY
            "reject" -> if (sdk >= 30) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.LONG_PRESS
            "toggle_on" -> if (sdk >= 34) HapticFeedbackConstants.TOGGLE_ON else HapticFeedbackConstants.CLOCK_TICK
            "toggle_off" -> if (sdk >= 34) HapticFeedbackConstants.TOGGLE_OFF else HapticFeedbackConstants.CLOCK_TICK
            else -> HapticFeedbackConstants.LONG_PRESS
        }
    }

    /** A session or the touchpad is up: screen stays on, no status or navigation bar. */
    private fun active(on: Boolean) {
        val c = WindowCompat.getInsetsController(window, window.decorView)
        if (on) {
            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            c.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            c.hide(WindowInsetsCompat.Type.systemBars())
        } else {
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            c.show(WindowInsetsCompat.Type.systemBars())
        }
    }
}
