package io.github.sandptel.wado

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

    override fun configureFlutterEngine(engine: FlutterEngine) {
        super.configureFlutterEngine(engine)
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
}
