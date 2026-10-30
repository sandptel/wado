package io.github.sandptel.wado.ui.pair

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.core.PairModel
import io.github.sandptel.wado.ui.common.WadoButton
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Type
import uniffi.wado_mobile_core.PairProgress

/** Where a pairing is, in plain words: a step with in-place progress, the check words, or why it failed. */
@Composable
fun PairStatus(pairing: PairModel?, idle: String, onRetry: () -> Unit) {
    val p = LocalPalette.current
    when (val s = pairing?.step) {
        null -> Text(idle, style = Type.body, color = p.dim)
        is PairProgress.CheckWords -> CheckWords(s.words)
        is PairProgress.Failed -> Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(s.why, style = Type.body, color = p.text, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
            WadoButton("Try again", onRetry, Modifier.fillMaxWidth(), primary = true)
        }
        else -> Row(verticalAlignment = Alignment.CenterVertically) {
            CircularProgressIndicator(Modifier.size(18.dp), color = p.accent, strokeWidth = 2.dp)
            Spacer(Modifier.width(10.dp))
            Text(
                when (s) {
                    PairProgress.Dialing -> "Reaching the computer…"
                    PairProgress.Securing -> "Securing the link…"
                    PairProgress.WaitingForComputer -> "Waiting for the computer to let this phone in…"
                    else -> "Pairing…"
                },
                style = Type.body,
                color = p.text,
                modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            )
        }
    }
}
