package io.github.sandptel.wado.core

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import uniffi.wado_mobile_core.SecretBox
import uniffi.wado_mobile_core.SecretException

/**
 * The core's secrets (this device's identity seed) wrapped by an AES-256-GCM key that lives in
 * AndroidKeyStore and never leaves it. No user-auth requirement: the core must run headless for
 * "Stay connected". Format: IV (12 bytes) ‖ ciphertext ‖ tag.
 */
class KeystoreSecretBox : SecretBox {
    private fun key(): SecretKey {
        val ks = KeyStore.getInstance(STORE).apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, STORE)
        gen.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return gen.generateKey()
    }

    override fun wrap(plain: ByteArray): ByteArray = guard {
        val c = Cipher.getInstance(MODE)
        c.init(Cipher.ENCRYPT_MODE, key())
        c.iv + c.doFinal(plain)
    }

    override fun unwrap(sealed: ByteArray): ByteArray = guard {
        require(sealed.size > IV) { "too short" }
        val c = Cipher.getInstance(MODE)
        c.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, sealed, 0, IV))
        c.doFinal(sealed, IV, sealed.size - IV)
    }

    private fun guard(f: () -> ByteArray): ByteArray =
        try {
            f()
        } catch (e: Exception) {
            throw SecretException.Failed(e.message ?: e.javaClass.simpleName)
        }

    private companion object {
        const val STORE = "AndroidKeyStore"
        const val ALIAS = "wado.core.secret"
        const val MODE = "AES/GCM/NoPadding"
        const val IV = 12
    }
}
