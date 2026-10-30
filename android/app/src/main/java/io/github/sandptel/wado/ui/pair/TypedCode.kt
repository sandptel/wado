package io.github.sandptel.wado.ui.pair

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import io.github.sandptel.wado.ui.phone.LocalSize
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.sandptel.wado.core.CoreHost
import io.github.sandptel.wado.ui.common.WadoButton
import io.github.sandptel.wado.ui.common.TopBar
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type
import uniffi.wado_mobile_core.PairProgress

/** The code, `XXXX-XXXX`, in mono at 20 sp: never under 16 sp on the glass at the smallest sizes. */
@Composable
fun CodeField(code: String, onChange: (String) -> Unit, enabled: Boolean = true) {
    // In a SizeScope a sp is (text scale × interface size) on the glass: never let that go under 16.
    val glass = LocalDensity.current.fontScale * LocalSize.current.ui
    val fs = maxOf(20f, 16f / glass)
    OutlinedTextField(
        code, { onChange(it.uppercase().take(9)) }, Modifier.fillMaxWidth().testTag("code"), label = { Text("Code") },
        singleLine = true, textStyle = Type.mono.copy(fontSize = fs.sp),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Characters, keyboardType = KeyboardType.Ascii),
        enabled = enabled,
    )
}

/** The public relay the web client starts with (`crates/client/src/state.rs` `DEFAULT_RELAY`). */
const val DEFAULT_RELAY = "https://wado-relay.indiasouthcentral.cloudapp.azure.com"

/**
 * Pairing by the code a computer shows (`wado pair`, `wado tui` → Pair): its relay, its Remote ID
 * and the eight-character code. Then the four check words, confirmed on the computer. Text is
 * 16 sp or more on the glass (no zoom-on-focus, and readable).
 */
@Composable
fun PairTypedScreen(core: CoreHost, onDone: () -> Unit, onBack: () -> Unit) {
    val p = LocalPalette.current
    val model by core.model.collectAsState()
    var relay by rememberSaveable { mutableStateOf(model.computers.firstOrNull()?.relayUrl ?: DEFAULT_RELAY) }
    var rid by rememberSaveable { mutableStateOf("") }
    var code by rememberSaveable { mutableStateOf("") }
    LaunchedEffect(model.justPaired) { if (model.justPaired != null) onDone() }
    DisposableEffect(Unit) { onDispose { core.clearPairing() } }
    val busy = model.pairing?.step.let { it != null && it !is PairProgress.Failed }
    val field = Type.body.copy(fontSize = maxOf(Type.body.fontSize.value, 16f).sp)

    Column(
        Modifier.fillMaxSize().background(p.ground).windowInsetsPadding(WindowInsets.safeDrawing).padding(16.dp)
            .verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        TopBar("Type a code", onBack)
        Column(
            Modifier.fillMaxWidth().widthIn(max = 520.dp).clip(RoundedCornerShape(Radius.r3)).background(p.surface1).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("On the computer, run wado pair (or wado tui → Pair). It shows a code like K7QF-2MXP.", style = Type.body, color = p.dim)
            OutlinedTextField(
                relay, { relay = it }, Modifier.fillMaxWidth(), label = { Text("Relay") }, singleLine = true, textStyle = field,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri), enabled = !busy,
            )
            OutlinedTextField(
                rid, { rid = it }, Modifier.fillMaxWidth(), label = { Text("Remote ID") }, singleLine = true, textStyle = field,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), enabled = !busy,
            )
            CodeField(code, { code = it }, enabled = !busy)
            if (!busy) {
                WadoButton("Pair", { core.pairTyped(relay, rid, code) }, Modifier.fillMaxWidth(), primary = true)
            }
            PairStatus(model.pairing, idle = "") { core.clearPairing() }
        }
    }
}

