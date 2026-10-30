package io.github.sandptel.wado.ui.phone

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.BuildConfig
import io.github.sandptel.wado.core.CoreHost
import io.github.sandptel.wado.nav.PhonePage
import io.github.sandptel.wado.ui.common.TopBar
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type

/**
 * The technical names, one tap from the plain words: each computer's link state and its relay's
 * caps, the core's version and this device's key, and how long the first live link took.
 */
@Composable
fun DiagnosticsScreen(core: CoreHost, onBack: () -> Unit) {
    val p = LocalPalette.current
    val model by core.model.collectAsState()
    val ready by core.core.collectAsState()
    val version = remember { runCatching { uniffi.wado_mobile_core.coreVersion() }.getOrElse { "not loaded" } }
    LazyColumn(
        Modifier.fillMaxSize().background(p.ground).windowInsetsPadding(WindowInsets.safeDrawing),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        item { TopBar(PhonePage.Diagnostics.title, onBack) }
        item {
            Card {
                Line("App", BuildConfig.VERSION_NAME)
                Line("Core", version + if (ready == null) " (not started${core.failure?.let { ": $it" } ?: ""})" else "")
                Line("This device", ready?.clientKey()?.take(8)?.plus("…") ?: "—")
                Line("First live link", core.firstOpenMs?.let { "$it ms after start" } ?: "not yet")
            }
        }
        items(model.computers.filter { !it.needsAppPairing }, key = { it.key }) { c ->
            Card {
                Text(c.name, style = Type.title, color = p.bright)
                Line("Remote ID", c.remoteId)
                Line("Relay", c.relayUrl)
                Line("Link", c.link.tech)
                Line("Daemon", c.link.instance.ifEmpty { "—" })
                Line("Words", c.link.words)
                Line("Relay caps", c.link.caps.joinToString(" ").ifEmpty { "—" })
            }
        }
    }
}

@Composable
private fun Card(content: @Composable () -> Unit) {
    val p = LocalPalette.current
    Column(
        Modifier.fillMaxWidth().widthIn(max = 720.dp).clip(RoundedCornerShape(Radius.r3)).background(p.surface1).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) { content() }
}

@Composable
private fun Line(k: String, v: String) {
    val p = LocalPalette.current
    Column {
        Text(k, style = Type.label, color = p.dim)
        Text(v, style = Type.mono, color = p.text)
    }
}
