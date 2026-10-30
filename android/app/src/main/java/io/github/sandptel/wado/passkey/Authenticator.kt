package io.github.sandptel.wado.passkey

import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import android.util.Log
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.core.content.edit
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import io.github.sandptel.wado.web.ClientServer
import uniffi.wado_mobile_core.webauthnAttestation
import uniffi.wado_mobile_core.webauthnAuthData
import uniffi.wado_mobile_core.webauthnClientData
import uniffi.wado_mobile_core.webauthnSigned
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.SecureRandom
import java.security.Signature
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec

/**
 * The app's own WebAuthn platform authenticator (js/native_passkey.js says why it needs one).
 *
 * Each passkey is a P-256 key made in the phone's Keystore — hardware-backed where the phone has
 * it — that signs only after the person's screen lock, every time (no validity window). The
 * private key never leaves the Keystore; the bytes around the signature come from the Rust core
 * (crates/mobile-core/src/webauthn.rs), tested against the computer's own verifier.
 *
 * **Every request is answered, once** (2026-10-30: a page waited for ever). Found on the
 * emulator: BiometricPrompt silently does nothing when asked while the activity is stopped
 * ("Called after onSaveInstanceState()"), and a second prompt while one is up leaves the first
 * caller's callback orphaned. So: one prompt at a time ([Ceremonies]; a newer one cancels the
 * older, which is told), a prompt asked for in the background waits for the activity to come
 * back, each has a deadline that cancels it, the page can cancel it, every failure says why
 * ([AuthErrors]), and nothing thrown is left unanswered.
 */
class Authenticator(private val activity: FragmentActivity) {
    sealed interface Result {
        data class Made(val credId: ByteArray, val clientData: ByteArray, val attestation: ByteArray) : Result
        data class Signed(val credId: ByteArray, val clientData: ByteArray, val authData: ByteArray, val signature: ByteArray) : Result
        data class Failed(val why: AuthFailure) : Result
    }

    private val prefs = activity.getSharedPreferences("passkeys", Context.MODE_PRIVATE)
    private val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
    private val main = Handler(Looper.getMainLooper())
    private val ceremonies = Ceremonies()

    /** The request now up: its id, how to answer it, and how to take its prompt down. */
    private class Live(val id: Int, val done: (Result) -> Unit) {
        var prompt: BiometricPrompt? = null
        var waiting: DefaultLifecycleObserver? = null
        var timeout: Runnable? = null
    }
    private val live = mutableMapOf<Int, Live>()

    fun create(id: Int, rpId: String, challenge: ByteArray, timeoutMs: Long, done: (Result) -> Unit) =
        begin(id, timeoutMs, done) { l -> makeAndProve(l, rpId, challenge) }

    fun get(id: Int, rpId: String, challenge: ByteArray, allow: List<ByteArray>, timeoutMs: Long, done: (Result) -> Unit) =
        begin(id, timeoutMs, done) { l -> signWith(l, rpId, challenge, allow) }

    /** The page gave up on `id` (Not now, a newer request, its own deadline). */
    fun cancel(id: Int) {
        val l = live[id] ?: return
        finish(l, Result.Failed(AuthErrors.cancelled()))
    }

    private fun begin(id: Int, timeoutMs: Long, done: (Result) -> Unit, run: (Live) -> Unit) {
        val l = Live(id, done)
        ceremonies.begin(id)?.let { old -> live[old]?.let { finish(it, Result.Failed(AuthErrors.cancelled())) } }
        live[id] = l
        val ms = timeoutMs.coerceIn(10_000, 120_000)
        l.timeout = Runnable {
            Log.w(TAG, "passkey: request $id unanswered after $ms ms — cancelled")
            finish(l, Result.Failed(AuthErrors.timeout(ms / 1000)))
        }.also { main.postDelayed(it, ms) }
        try {
            run(l)
        } catch (e: Exception) {
            Log.w(TAG, "passkey: request $id failed", e)
            finish(l, Result.Failed(AuthErrors.failed(e.message)))
        }
    }

    /** Answer `l` — once — and take down whatever it had up. */
    private fun finish(l: Live, r: Result) {
        if (!ceremonies.finish(l.id)) return
        live.remove(l.id)
        l.timeout?.let(main::removeCallbacks)
        l.waiting?.let { activity.lifecycle.removeObserver(it) }
        l.prompt?.let { p -> runCatching { p.cancelAuthentication() } }
        l.done(r)
    }

    private fun makeAndProve(l: Live, rpId: String, challenge: ByteArray) {
        if (rpId != ClientServer.DOMAIN) return finish(l, Result.Failed(AuthFailure("SecurityError", AuthErrors.FAILED, "Not this app's page.")))
        AuthErrors.fromCanAuthenticate(canAuthenticate())?.let { return finish(l, Result.Failed(it)) }
        val credId = ByteArray(16).also(SecureRandom()::nextBytes)
        val alias = alias(credId)
        val pub = try {
            makeKey(alias)
        } catch (e: Exception) {
            return finish(
                l,
                Result.Failed(
                    AuthFailure(
                        "NotSupportedError", AuthErrors.NO_SCREEN_LOCK,
                        "This phone has no screen lock to protect a passkey with. Set a PIN, fingerprint or face unlock, then try again — or approve this phone on the computer.",
                    ),
                ),
            )
        }
        // Using it once proves the screen lock now, as a passkey's creation must.
        sign(l, alias, "Add a passkey for wado", ByteArray(32).also(SecureRandom()::nextBytes)) { sig ->
            val point = uncompressed(pub)
            val clientData = webauthnClientData(true, challenge, ClientServer.ORIGIN)
            val att = runCatching { webauthnAttestation(rpId, credId, point) }.getOrElse {
                return@sign finish(l, Result.Failed(AuthErrors.failed(it.message)))
            }
            remember(rpId, credId)
            finish(l, Result.Made(credId, clientData, att))
        }.also { ok -> if (!ok) runCatching { store.deleteEntry(alias) } }
    }

    private fun signWith(l: Live, rpId: String, challenge: ByteArray, allow: List<ByteArray>) {
        if (rpId != ClientServer.DOMAIN) return finish(l, Result.Failed(AuthFailure("SecurityError", AuthErrors.FAILED, "Not this app's page.")))
        val held = known(rpId)
        // Fail at once, saying why: a passkey made in a browser (another site, another key) can
        // never be answered here, and a prompt that cannot succeed only teaches "try again".
        val credId = (if (allow.isEmpty()) held.lastOrNull() else allow.firstOrNull { a -> held.any { it.contentEquals(a) } })
            ?: return finish(l, Result.Failed(AuthErrors.notHeld()))
        AuthErrors.fromCanAuthenticate(canAuthenticate())?.let { return finish(l, Result.Failed(it)) }
        val clientData = webauthnClientData(false, challenge, ClientServer.ORIGIN)
        val authData = webauthnAuthData(rpId, 0u)
        sign(l, alias(credId), "Unlock wado", webauthnSigned(authData, clientData)) { sig ->
            finish(l, Result.Signed(credId, clientData, authData, sig))
        }
    }

    private fun canAuthenticate(): Int =
        if (Build.VERSION.SDK_INT >= 30) BiometricManager.from(activity).canAuthenticate(BIOMETRIC_STRONG or DEVICE_CREDENTIAL)
        else BiometricManager.from(activity).canAuthenticate(BIOMETRIC_STRONG)

    private fun makeKey(alias: String): ECPublicKey {
        val spec = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_SIGN)
            .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
            .setDigests(KeyProperties.DIGEST_SHA256)
            .setUserAuthenticationRequired(true)
            .apply {
                if (Build.VERSION.SDK_INT >= 30) {
                    setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL)
                } else {
                    @Suppress("DEPRECATION")
                    setUserAuthenticationValidityDurationSeconds(-1)
                }
            }
            .build()
        val gen = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, KEYSTORE)
        gen.initialize(spec)
        return gen.generateKeyPair().public as ECPublicKey
    }

    /**
     * Sign `data` once the screen lock is confirmed, then `signed`; a failure finishes `l` with
     * its reason. False when it could not even start (the key is gone or unusable).
     */
    private fun sign(l: Live, alias: String, title: String, data: ByteArray, signed: (ByteArray) -> Unit): Boolean {
        val key = store.getKey(alias, null) as? PrivateKey
        if (key == null) {
            finish(l, Result.Failed(AuthErrors.notHeld()))
            return false
        }
        val s = Signature.getInstance("SHA256withECDSA")
        try {
            s.initSign(key)
        } catch (e: KeyPermanentlyInvalidatedException) {
            finish(l, Result.Failed(AuthErrors.invalidated()))
            return false
        } catch (e: Exception) {
            finish(l, Result.Failed(AuthErrors.failed(e.message)))
            return false
        }
        val info = BiometricPrompt.PromptInfo.Builder()
            .setTitle(title)
            .setSubtitle("Confirm with your screen lock")
            .apply {
                if (Build.VERSION.SDK_INT >= 30) setAllowedAuthenticators(BIOMETRIC_STRONG or DEVICE_CREDENTIAL)
                else setAllowedAuthenticators(BIOMETRIC_STRONG).setNegativeButtonText("Cancel")
            }
            .build()
        val show = show@{
            if (!ceremonies.isCurrent(l.id)) return@show
            val prompt = BiometricPrompt(
                activity,
                ContextCompat.getMainExecutor(activity),
                object : BiometricPrompt.AuthenticationCallback() {
                    override fun onAuthenticationSucceeded(r: BiometricPrompt.AuthenticationResult) {
                        val sig = r.cryptoObject?.signature
                        val out = sig?.let { runCatching { it.update(data); it.sign() }.getOrNull() }
                        if (out == null) finish(l, Result.Failed(AuthErrors.failed("the signature could not be made")))
                        else signed(out)
                    }

                    override fun onAuthenticationError(code: Int, msg: CharSequence) {
                        Log.i(TAG, "passkey: prompt error $code $msg")
                        finish(l, Result.Failed(AuthErrors.fromPrompt(code, msg)))
                    }
                },
            )
            l.prompt = prompt
            prompt.authenticate(info, BiometricPrompt.CryptoObject(s))
        }
        // BiometricPrompt does nothing — and never answers — when asked after the activity saved
        // its state. Asked in the background, it waits for the activity to come back (within
        // the deadline).
        if (activity.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) && !activity.supportFragmentManager.isStateSaved) {
            show()
        } else {
            Log.i(TAG, "passkey: request ${l.id} waits for wado to be on screen")
            val obs = object : DefaultLifecycleObserver {
                override fun onResume(owner: LifecycleOwner) {
                    activity.lifecycle.removeObserver(this)
                    l.waiting = null
                    main.post(show)
                }
            }
            l.waiting = obs
            activity.lifecycle.addObserver(obs)
        }
        return true
    }

    private fun known(rpId: String): List<ByteArray> =
        prefs.getString(rpId, "").orEmpty().split(',').filter { it.isNotBlank() }.map(::dec)
            .filter { store.containsAlias(alias(it)) }

    private fun remember(rpId: String, credId: ByteArray) = prefs.edit {
        putString(rpId, (known(rpId).map(::enc) + enc(credId)).joinToString(","))
    }

    companion object {
        private const val KEYSTORE = "AndroidKeyStore"
        private const val TAG = "wado-passkey"

        fun enc(b: ByteArray): String = Base64.encodeToString(b, Base64.URL_SAFE or Base64.NO_PADDING or Base64.NO_WRAP)
        fun dec(s: String): ByteArray = Base64.decode(s, Base64.URL_SAFE or Base64.NO_PADDING or Base64.NO_WRAP)
        private fun alias(credId: ByteArray) = "wado-passkey-" + enc(credId)

        /** `04 ‖ x ‖ y`, each coordinate left-padded to 32 bytes. */
        fun uncompressed(k: ECPublicKey): ByteArray {
            fun fixed(v: java.math.BigInteger): ByteArray {
                val b = v.toByteArray().let { if (it.size > 32) it.copyOfRange(it.size - 32, it.size) else it }
                return ByteArray(32 - b.size) + b
            }
            return byteArrayOf(4) + fixed(k.w.affineX) + fixed(k.w.affineY)
        }
    }
}
