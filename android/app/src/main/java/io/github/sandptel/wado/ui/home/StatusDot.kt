package io.github.sandptel.wado.ui.home

import androidx.compose.runtime.Composable
import io.github.sandptel.wado.online.OnlineProbe.Seen
import uniffi.wado_mobile_core.LinkTone

/**
 * A web-known computer's status from the relay's `/online/:id` (the share sheet's picker, until
 * the share sheet moves onto the core in P4), drawn with the core cards' tone dot.
 */
fun status(seen: Seen) = when (seen) {
    Seen.ONLINE -> "Online"
    Seen.OFFLINE -> "Offline"
    Seen.UNREACHABLE -> "Can't reach its relay"
    Seen.UNKNOWN -> "Checking…"
}

@Composable
fun StatusDot(seen: Seen) = ToneDot(
    when (seen) {
        Seen.ONLINE -> LinkTone.LIVE
        Seen.OFFLINE -> LinkTone.STILL
        Seen.UNREACHABLE -> LinkTone.BAD
        Seen.UNKNOWN -> LinkTone.BUSY
    },
)
