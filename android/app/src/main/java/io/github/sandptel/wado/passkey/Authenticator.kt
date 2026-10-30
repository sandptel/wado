package io.github.sandptel.wado.passkey

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.core.content.edit
import androidx.fragment.app.FragmentActivity
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
 */
class Authenticator(private val activity: FragmentActivity) {
    sealed interface Result {
        data class Made(val credId: ByteArray, val clientData: ByteArray, val attestation: ByteArray) : Result
        data class Signed(val credId: ByteArray, val clientData: ByteArray, val authData: ByteArray, val signature: ByteArray) : Result
        data class Failed(val name: String, val message: String) : Result
    }

    private val prefs = activity.getSharedPreferences("passkeys", Context.MODE_PRIVATE)
    private val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }

    fun create(rpId: String, challenge: ByteArray, done: (Result) -> Unit) {
        if (rpId != ClientServer.DOMAIN) return done(Result.Failed("SecurityError", "Not this app's page."))
        val credId = ByteArray(16).also(SecureRandom()::nextBytes)
        val alias = alias(credId)
        val pub = try {
            makeKey(alias)
        } catch (e: Exception) {
            return done(
                Result.Failed(
                    "NotSupportedError",
                    "This phone has no screen lock to protect a passkey with. Set a PIN, fingerprint or face unlock, then try again.",
                ),
            )
        }
        // Using it once proves the screen lock now, as a passkey's creation must.
        sign(alias, "Add a passkey for wado", ByteArray(32).also(SecureRandom()::nextBytes)) { sig ->
            if (sig == null) {
                runCatching { store.deleteEntry(alias) }
                return@sign done(Result.Failed("NotAllowedError", "The screen lock wasn't confirmed."))
            }
            val point = uncompressed(pub)
            val clientData = webauthnClientData(true, challenge, ClientServer.ORIGIN)
            val att = runCatching { webauthnAttestation(rpId, credId, point) }.getOrElse {
                return@sign done(Result.Failed("UnknownError", it.message ?: "The passkey could not be laid out."))
            }
            remember(rpId, credId)
            done(Result.Made(credId, clientData, att))
        }
    }

    fun get(rpId: String, challenge: ByteArray, allow: List<ByteArray>, done: (Result) -> Unit) {
        if (rpId != ClientServer.DOMAIN) return done(Result.Failed("SecurityError", "Not this app's page."))
        val held = known(rpId)
        val credId = (if (allow.isEmpty()) held.lastOrNull() else allow.firstOrNull { a -> held.any { it.contentEquals(a) } })
            ?: return done(Result.Failed("NotAllowedError", "This phone doesn't hold that passkey."))
        val clientData = webauthnClientData(false, challenge, ClientServer.ORIGIN)
        val authData = webauthnAuthData(rpId, 0u)
        sign(alias(credId), "Unlock wado", webauthnSigned(authData, clientData)) { sig ->
            if (sig == null) done(Result.Failed("NotAllowedError", "The screen lock wasn't confirmed."))
            else done(Result.Signed(credId, clientData, authData, sig))
        }
    }

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

    /** Sign `data` once the screen lock is confirmed; null when it was not. */
    private fun sign(alias: String, title: String, data: ByteArray, done: (ByteArray?) -> Unit) {
        val key = store.getKey(alias, null) as? PrivateKey ?: return done(null)
        val s = Signature.getInstance("SHA256withECDSA")
        try {
            s.initSign(key)
        } catch (_: Exception) {
            return done(null)
        }
        val info = BiometricPrompt.PromptInfo.Builder()
            .setTitle(title)
            .setSubtitle("Confirm with your screen lock")
            .apply {
                if (Build.VERSION.SDK_INT >= 30) setAllowedAuthenticators(BIOMETRIC_STRONG or DEVICE_CREDENTIAL)
                else setAllowedAuthenticators(BIOMETRIC_STRONG).setNegativeButtonText("Cancel")
            }
            .build()
        val prompt = BiometricPrompt(
            activity,
            ContextCompat.getMainExecutor(activity),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(r: BiometricPrompt.AuthenticationResult) {
                    val sig = r.cryptoObject?.signature ?: return done(null)
                    done(runCatching { sig.update(data); sig.sign() }.getOrNull())
                }

                override fun onAuthenticationError(code: Int, msg: CharSequence) = done(null)
            },
        )
        prompt.authenticate(info, BiometricPrompt.CryptoObject(s))
    }

    private fun known(rpId: String): List<ByteArray> =
        prefs.getString(rpId, "").orEmpty().split(',').filter { it.isNotBlank() }.map(::dec)
            .filter { store.containsAlias(alias(it)) }

    private fun remember(rpId: String, credId: ByteArray) = prefs.edit {
        putString(rpId, (known(rpId).map(::enc) + enc(credId)).joinToString(","))
    }

    companion object {
        private const val KEYSTORE = "AndroidKeyStore"

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
