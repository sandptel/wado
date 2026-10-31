package io.github.sandptel.wado

import android.Manifest
import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Intent
import android.os.PersistableBundle
import android.content.pm.PackageManager
import android.os.Build
import android.view.HapticFeedbackConstants
import android.view.WindowManager
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.activity.result.contract.ActivityResultContracts
import io.flutter.embedding.android.FlutterFragmentActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import io.github.sandptel.wado.passkey.AuthErrors
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

    private var os: MethodChannel? = null

    // A new intent while running goes straight to Dart; the launching one waits until Dart asks.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        Inbound.of(intent, contentResolver)?.let { os?.invokeMethod("intent", it) }
    }

    override fun configureFlutterEngine(engine: FlutterEngine) {
        super.configureFlutterEngine(engine)
        os = MethodChannel(engine.dartExecutor.binaryMessenger, "wado/os")
        os!!.setMethodCallHandler { call, result ->
            when (call.method) {
                "intent" -> {
                    result.success(Inbound.of(intent, contentResolver))
                    intent = Intent() // answered once: a recreated activity does not replay it
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
                "haptic" -> {
                    window.decorView.performHapticFeedback(haptic(call.argument<String>("kind").orEmpty()))
                    result.success(null)
                }
                "active" -> {
                    active(call.argument<Boolean>("on") == true)
                    result.success(null)
                }
                "shortcut" -> {
                    Shortcut.desktop(this, call.argument<String>("key").orEmpty(), call.argument<String>("title").orEmpty())
                    result.success(null)
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
