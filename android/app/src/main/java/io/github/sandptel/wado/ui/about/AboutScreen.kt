package io.github.sandptel.wado.ui.about

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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import io.github.sandptel.wado.BuildConfig
import io.github.sandptel.wado.ui.common.WadoButton
import io.github.sandptel.wado.ui.home.Wordmark
import io.github.sandptel.wado.ui.settings.TopBar
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type

const val SOURCE_URL = "https://github.com/sandptel/wado"

/**
 * What this app is and the terms it comes under. wado is AGPL-3.0-only: the licence's full text
 * and the source are one tap away, as section 13 asks of a program used over a network.
 */
@Composable
fun AboutScreen(onOpenUrl: (String) -> Unit, onBack: () -> Unit) {
    val p = LocalPalette.current
    val context = LocalContext.current
    var shown by remember { mutableStateOf<String?>(null) }
    val read = { name: String ->
        runCatching { context.assets.open("licenses/$name").bufferedReader().use { it.readText() } }
            .getOrDefault("This build carries no copy of $name; it is in the source at $SOURCE_URL.")
    }
    LazyColumn(
        Modifier.fillMaxSize().background(p.ground).windowInsetsPadding(WindowInsets.safeDrawing),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { TopBar("About", onBack) }
        item {
            Column(
                Modifier.fillMaxWidth().widthIn(max = 720.dp).clip(RoundedCornerShape(Radius.r3)).background(p.surface1).padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Wordmark()
                Text("Version ${BuildConfig.VERSION_NAME}", style = Type.mono, color = p.dim)
                Text(
                    "Your computers, at this screen's own resolution. wado is free software: you can redistribute and modify it under the GNU Affero General Public License, version 3 only. It comes with no warranty.",
                    style = Type.body,
                    color = p.text,
                )
                Text(
                    "This app bundles the same client the browser runs, and updates it only when the app itself updates.",
                    style = Type.small,
                    color = p.dim,
                )
            }
        }
        item { WadoButton("Source code", { onOpenUrl(SOURCE_URL) }, Modifier.fillMaxWidth(), primary = true) }
        item { WadoButton("GNU AGPL-3.0 — full text", { shown = if (shown == "AGPL-3.0.txt") null else "AGPL-3.0.txt" }, Modifier.fillMaxWidth()) }
        item {
            WadoButton("Fonts — SIL Open Font License", {
                shown = if (shown == "fonts") null else "fonts"
            }, Modifier.fillMaxWidth())
        }
        item {
            Text(
                "Uses ZXing (Apache-2.0) to read QR codes and the UnifiedPush connector (Apache-2.0) for push without Google.",
                style = Type.small,
                color = p.dim,
                modifier = Modifier.padding(horizontal = 6.dp),
            )
        }
        shown?.let { s ->
            item {
                val text = if (s == "fonts") {
                    listOf("OFL-BricolageGrotesque.txt", "OFL-Figtree.txt", "OFL-JetBrainsMono.txt").joinToString("\n\n————\n\n") { read(it) }
                } else read(s)
                Text(
                    text,
                    style = Type.mono,
                    color = p.text,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(Radius.r2)).background(p.surface1).padding(12.dp),
                )
            }
        }
    }
}
