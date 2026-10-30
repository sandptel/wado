package io.github.sandptel.wado.passkey

import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt

/**
 * Why the app's passkey did not answer, in words for a person and a code for the page
 * (`js/passkey_errors.js` reads `code` to decide what the sheet offers next). Pure: tested on
 * the JVM (`AuthErrorsTest`).
 *
 * Every failure names a next step — "try again" alone is how a passkey that can never work here
 * kept a person trying (2026-10-30).
 */
data class AuthFailure(val name: String, val code: String, val message: String)

object AuthErrors {
    const val CANCELLED = "cancelled"
    const val TIMEOUT = "timeout"
    const val LOCKOUT = "lockout"
    const val LOCKOUT_PERMANENT = "lockout_permanent"
    const val NO_SCREEN_LOCK = "no_screen_lock"
    const val NONE_ENROLLED = "none_enrolled"
    const val HARDWARE = "hardware"
    const val NOT_HELD = "not_held"
    const val INVALIDATED = "invalidated"
    const val BUSY = "busy"
    const val FAILED = "failed"

    private const val ELSEWHERE = "Approve this phone on the computer instead, or ask its owner to stop asking this phone for a passkey."

    fun cancelled() = AuthFailure("NotAllowedError", CANCELLED, "Cancelled — tap Unlock when you're ready.")

    fun timeout(seconds: Long) = AuthFailure(
        "NotAllowedError", TIMEOUT,
        "The screen lock didn't answer in $seconds seconds. Try again, or approve this phone on the computer instead.",
    )

    fun notHeld() = AuthFailure(
        "NotAllowedError", NOT_HELD,
        "This app holds no passkey for this computer — yours was made in a browser, or before the app was reinstalled. " +
            "Approve this phone on the computer, then add a passkey for the app.",
    )

    fun invalidated() = AuthFailure(
        "NotAllowedError", INVALIDATED,
        "This phone's passkey stopped working because its screen lock or fingerprints changed. " +
            "Approve this phone on the computer, then add a new passkey.",
    )

    fun busy() = AuthFailure("NotAllowedError", BUSY, "Open wado on the phone's screen to use your passkey, then try again.")

    fun failed(why: String?) = AuthFailure(
        "UnknownError", FAILED,
        "The passkey didn't work" + (why?.takeIf { it.isNotBlank() }?.let { ": $it" } ?: ".") + " Try again, or approve this phone on the computer.",
    )

    /** What [BiometricManager.canAuthenticate] answered, before any prompt: null when it can. */
    fun fromCanAuthenticate(result: Int): AuthFailure? = when (result) {
        BiometricManager.BIOMETRIC_SUCCESS -> null
        BiometricManager.BIOMETRIC_ERROR_NONE_ENROLLED -> AuthFailure(
            "NotSupportedError", NO_SCREEN_LOCK,
            "This phone has no screen lock. Set a PIN, pattern, fingerprint or face unlock in the phone's settings, then try again — or $ELSEWHERE",
        )
        BiometricManager.BIOMETRIC_ERROR_HW_UNAVAILABLE,
        BiometricManager.BIOMETRIC_ERROR_NO_HARDWARE,
        BiometricManager.BIOMETRIC_ERROR_SECURITY_UPDATE_REQUIRED,
        -> AuthFailure(
            "NotAllowedError", HARDWARE,
            "The phone's fingerprint and screen-lock check isn't available right now. Try again in a moment — or $ELSEWHERE",
        )
        // Unknown or unsupported: let the prompt itself say.
        else -> null
    }

    /** What BiometricPrompt's `onAuthenticationError` reported. */
    fun fromPrompt(code: Int, said: CharSequence?): AuthFailure = when (code) {
        BiometricPrompt.ERROR_USER_CANCELED,
        BiometricPrompt.ERROR_NEGATIVE_BUTTON,
        BiometricPrompt.ERROR_CANCELED,
        -> cancelled()
        BiometricPrompt.ERROR_TIMEOUT -> AuthFailure(
            "NotAllowedError", TIMEOUT,
            "The fingerprint check timed out. Try again, or use your PIN.",
        )
        BiometricPrompt.ERROR_LOCKOUT -> AuthFailure(
            "NotAllowedError", LOCKOUT,
            "Too many tries. Wait 30 seconds and try again, or use your PIN.",
        )
        BiometricPrompt.ERROR_LOCKOUT_PERMANENT -> AuthFailure(
            "NotAllowedError", LOCKOUT_PERMANENT,
            "Fingerprint is locked after too many tries. Unlock the phone with its PIN once, then try again — or $ELSEWHERE",
        )
        BiometricPrompt.ERROR_NO_BIOMETRICS -> AuthFailure(
            "NotAllowedError", NONE_ENROLLED,
            "No fingerprint or face is set up. Add one in the phone's settings, use your PIN, or $ELSEWHERE",
        )
        BiometricPrompt.ERROR_NO_DEVICE_CREDENTIAL -> AuthFailure(
            "NotSupportedError", NO_SCREEN_LOCK,
            "This phone has no screen lock. Set a PIN, pattern, fingerprint or face unlock, then try again — or $ELSEWHERE",
        )
        BiometricPrompt.ERROR_HW_UNAVAILABLE,
        BiometricPrompt.ERROR_HW_NOT_PRESENT,
        BiometricPrompt.ERROR_UNABLE_TO_PROCESS,
        BiometricPrompt.ERROR_NO_SPACE,
        BiometricPrompt.ERROR_VENDOR,
        -> AuthFailure(
            "NotAllowedError", HARDWARE,
            "The phone's fingerprint check isn't working right now" +
                (said?.toString()?.takeIf { it.isNotBlank() }?.let { " ($it)" } ?: "") +
                ". Try again, use your PIN, or $ELSEWHERE",
        )
        else -> failed(said?.toString())
    }
}

/**
 * One prompt at a time, and each answered exactly once: a newer one replaces the old (whose
 * caller is told), and whichever comes first of the result, the deadline and a cancel wins.
 * An orphaned request is how a page waited for ever (2026-10-30).
 */
class Ceremonies {
    private var current: Int? = null
    private val done = mutableSetOf<Int>()

    /** `id` begins; returns the one it replaces, still unanswered, if any. */
    fun begin(id: Int): Int? {
        val old = current?.takeIf { it != id && it !in done }
        current = id
        return old
    }

    /** May `id` be answered now? True once only. */
    fun finish(id: Int): Boolean {
        if (id in done) return false
        done += id
        if (current == id) current = null
        // Keep the set small: only recent ids can still race.
        if (done.size > 64) done.remove(done.min())
        return true
    }

    fun isCurrent(id: Int) = current == id
}
