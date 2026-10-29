package io.github.sandptel.wado.device

import android.os.Build
import android.view.HapticFeedbackConstants
import android.view.View

/**
 * The page's haptic kinds as the phone's own predefined effects — the system's feel, and its
 * "touch feedback" switch respected — rather than a length of motor buzz.
 */
object Haptics {
    fun constant(kind: String, sdk: Int = Build.VERSION.SDK_INT): Int = when (kind) {
        "tick" -> if (sdk >= 34) HapticFeedbackConstants.SEGMENT_FREQUENT_TICK else HapticFeedbackConstants.CLOCK_TICK
        "tap" -> HapticFeedbackConstants.VIRTUAL_KEY
        "claim" -> if (sdk >= 30) HapticFeedbackConstants.GESTURE_START else HapticFeedbackConstants.LONG_PRESS
        "confirm" -> if (sdk >= 30) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.VIRTUAL_KEY
        "reject" -> if (sdk >= 30) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.LONG_PRESS
        "toggle_on" -> if (sdk >= 34) HapticFeedbackConstants.TOGGLE_ON else HapticFeedbackConstants.CLOCK_TICK
        "toggle_off" -> if (sdk >= 34) HapticFeedbackConstants.TOGGLE_OFF else HapticFeedbackConstants.CLOCK_TICK
        else -> HapticFeedbackConstants.LONG_PRESS
    }

    fun perform(view: View, kind: String) {
        view.performHapticFeedback(constant(kind))
    }
}
