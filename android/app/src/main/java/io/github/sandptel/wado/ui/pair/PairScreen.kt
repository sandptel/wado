package io.github.sandptel.wado.ui.pair

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import io.github.sandptel.wado.links.DeepLinks
import io.github.sandptel.wado.ui.common.LineIcon
import io.github.sandptel.wado.ui.common.Pressable
import io.github.sandptel.wado.ui.common.WadoButton
import io.github.sandptel.wado.ui.common.semanticsLabel
import io.github.sandptel.wado.ui.theme.LocalPalette
import io.github.sandptel.wado.ui.theme.Motion
import io.github.sandptel.wado.ui.theme.Radius
import io.github.sandptel.wado.ui.theme.Type
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * "Pair this phone": the computer's QR, scanned with the camera and decoded here; or, without a
 * camera (or a code too far away), the typed code in the client's own form. A code that is not
 * a wado link shakes the frame and says so, and scanning goes on.
 */
@Composable
fun PairScreen(onLink: (Map<String, String>) -> Unit, onTypeCode: () -> Unit, onBack: () -> Unit) {
    val p = LocalPalette.current
    val context = LocalContext.current
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    var granted by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    var asked by remember { mutableStateOf(false) }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted = it; asked = true }
    LaunchedEffect(Unit) { if (!granted) ask.launch(Manifest.permission.CAMERA) }

    var note by remember { mutableStateOf("Point the camera at the code your computer shows.") }
    val found = remember { Animatable(0f) }
    val shake = remember { Animatable(0f) }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        if (granted) {
            QrScanner(
                accept = { text ->
                    val link = DeepLinks.connect(text)
                    if (link == null) {
                        haptics.performHapticFeedback(HapticFeedbackType.Reject)
                        note = "That code isn't a wado pairing code."
                        scope.launch {
                            for (x in listOf(10f, -8f, 5f, 0f)) shake.animateTo(x, Motion.spring(70))
                        }
                        false
                    } else {
                        haptics.performHapticFeedback(HapticFeedbackType.Confirm)
                        note = if (DeepLinks.pairs(link)) "Found it — pairing…" else "Found it — connecting…"
                        scope.launch {
                            found.animateTo(1f, Motion.pop(Motion.DUR))
                            delay(260)
                            onLink(link)
                        }
                        true
                    }
                },
                modifier = Modifier.fillMaxSize(),
            )
        }
        Column(
            Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).padding(16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Pressable(onBack, Modifier.size(44.dp).semanticsLabel("Back"), color = Color.Black.copy(alpha = 0.4f), radius = 22.dp, sink = 0.9f) {
                    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { LineIcon("back", Color.White) }
                }
            }
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Viewfinder(
                    color = if (found.value > 0f) p.good else p.accent,
                    modifier = Modifier
                        .widthIn(max = 320.dp).fillMaxWidth(0.72f).aspectRatio(1f)
                        .graphicsLayer {
                            translationX = shake.value.dp.toPx()
                            val s = 1f + 0.06f * found.value
                            scaleX = s; scaleY = s
                        },
                )
            }
            Column(
                Modifier.fillMaxWidth().widthIn(max = 520.dp).clip(RoundedCornerShape(Radius.r3)).background(p.surface1).padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text("Pair this phone", style = Type.headline, color = p.bright)
                Text(
                    when {
                        granted -> note
                        asked -> "wado can't use the camera, so type the code instead — or allow the camera in Android's settings."
                        else -> "wado asks for the camera only to read this code."
                    },
                    style = Type.body,
                    color = p.dim,
                    textAlign = TextAlign.Start,
                )
                WadoButton("Type a code instead", onTypeCode, Modifier.fillMaxWidth())
            }
        }
    }
}

/** Four rounded corners of a square, the code's target. */
@Composable
private fun Viewfinder(color: Color, modifier: Modifier) {
    Canvas(modifier) {
        val w = size.width
        val arm = w * 0.18f
        val r = 22.dp.toPx()
        val stroke = Stroke(width = 4.dp.toPx(), cap = StrokeCap.Round)
        // Each corner: a quarter round with two short arms.
        listOf(Offset(0f, 0f) to 180f, Offset(w - 2 * r, 0f) to 270f, Offset(w - 2 * r, w - 2 * r) to 0f, Offset(0f, w - 2 * r) to 90f)
            .forEach { (o, start) -> drawArc(color, start, 90f, false, o, Size(2 * r, 2 * r), style = stroke) }
        drawLine(color, Offset(r, 0f), Offset(arm, 0f), stroke.width, StrokeCap.Round)
        drawLine(color, Offset(0f, r), Offset(0f, arm), stroke.width, StrokeCap.Round)
        drawLine(color, Offset(w - arm, 0f), Offset(w - r, 0f), stroke.width, StrokeCap.Round)
        drawLine(color, Offset(w, r), Offset(w, arm), stroke.width, StrokeCap.Round)
        drawLine(color, Offset(w - arm, w), Offset(w - r, w), stroke.width, StrokeCap.Round)
        drawLine(color, Offset(w, w - r), Offset(w, w - arm), stroke.width, StrokeCap.Round)
        drawLine(color, Offset(r, w), Offset(arm, w), stroke.width, StrokeCap.Round)
        drawLine(color, Offset(0f, w - r), Offset(0f, w - arm), stroke.width, StrokeCap.Round)
        drawRoundRect(color.copy(alpha = 0.06f), cornerRadius = CornerRadius(r))
    }
}
