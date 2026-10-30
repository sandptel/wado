package io.github.sandptel.wado.passkey

import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AuthErrorsTest {
    @Test fun everyPromptErrorHasACodeAndANextStep() {
        val codes = listOf(
            BiometricPrompt.ERROR_USER_CANCELED to AuthErrors.CANCELLED,
            BiometricPrompt.ERROR_NEGATIVE_BUTTON to AuthErrors.CANCELLED,
            BiometricPrompt.ERROR_CANCELED to AuthErrors.CANCELLED,
            BiometricPrompt.ERROR_TIMEOUT to AuthErrors.TIMEOUT,
            BiometricPrompt.ERROR_LOCKOUT to AuthErrors.LOCKOUT,
            BiometricPrompt.ERROR_LOCKOUT_PERMANENT to AuthErrors.LOCKOUT_PERMANENT,
            BiometricPrompt.ERROR_NO_BIOMETRICS to AuthErrors.NONE_ENROLLED,
            BiometricPrompt.ERROR_NO_DEVICE_CREDENTIAL to AuthErrors.NO_SCREEN_LOCK,
            BiometricPrompt.ERROR_HW_UNAVAILABLE to AuthErrors.HARDWARE,
            BiometricPrompt.ERROR_HW_NOT_PRESENT to AuthErrors.HARDWARE,
            BiometricPrompt.ERROR_VENDOR to AuthErrors.HARDWARE,
            999 to AuthErrors.FAILED,
        )
        for ((code, want) in codes) {
            val f = AuthErrors.fromPrompt(code, "said")
            assertEquals("code $code", want, f.code)
            assertTrue("a sentence for $code", f.message.length > 20)
        }
    }

    @Test fun whatCannotWorkHereSaysApproveOnTheComputer() {
        for (f in listOf(
            AuthErrors.notHeld(),
            AuthErrors.invalidated(),
            AuthErrors.fromPrompt(BiometricPrompt.ERROR_LOCKOUT_PERMANENT, null),
            AuthErrors.fromPrompt(BiometricPrompt.ERROR_NO_DEVICE_CREDENTIAL, null),
        )) {
            assertTrue(f.message, f.message.contains("on the computer"))
        }
        assertTrue(AuthErrors.notHeld().message.contains("browser"))
    }

    @Test fun canAuthenticateBeforeAnyPrompt() {
        assertNull(AuthErrors.fromCanAuthenticate(BiometricManager.BIOMETRIC_SUCCESS))
        assertEquals(AuthErrors.NO_SCREEN_LOCK, AuthErrors.fromCanAuthenticate(BiometricManager.BIOMETRIC_ERROR_NONE_ENROLLED)?.code)
        assertEquals(AuthErrors.HARDWARE, AuthErrors.fromCanAuthenticate(BiometricManager.BIOMETRIC_ERROR_HW_UNAVAILABLE)?.code)
        assertNull("unknown: the prompt says", AuthErrors.fromCanAuthenticate(BiometricManager.BIOMETRIC_STATUS_UNKNOWN))
    }

    @Test fun theTimeoutSaysHowLong() {
        val t = AuthErrors.timeout(60)
        assertEquals(AuthErrors.TIMEOUT, t.code)
        assertTrue(t.message.contains("60 seconds"))
    }
}

class CeremoniesTest {
    @Test fun aNewPromptReplacesTheOldAndTheOldIsAnswered() {
        val c = Ceremonies()
        assertNull(c.begin(1))
        assertEquals("the orphan is named, to be told", 1, c.begin(2))
        assertTrue(c.finish(1))
        assertTrue(c.isCurrent(2))
        assertTrue(c.finish(2))
        assertFalse(c.isCurrent(2))
    }

    @Test fun eachIsAnsweredOnceWhicheverComesFirst() {
        val c = Ceremonies()
        c.begin(7)
        assertTrue("the deadline", c.finish(7))
        assertFalse("the late result is dropped", c.finish(7))
        assertNull("finished: nothing to replace", c.begin(8))
    }
}
