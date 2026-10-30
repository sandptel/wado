package io.github.sandptel.wado.pad

import android.os.SystemClock
import io.github.sandptel.wado.bridge.BridgeProtocol
import io.github.sandptel.wado.bridge.Hub
import org.json.JSONObject
import uniffi.wado_mobile_core.PadOut
import uniffi.wado_mobile_core.PadPacer

/**
 * The native trackpad's sending side: pointers go into the Rust pacer (crates/mobile-core, the
 * same pacing as the web pad), and what it says to send goes out on the page's pad link
 * (js/native_pad.js) — relayed through the envelope, or direct.
 */
class PadLink(private val key: String) {
    private val pacer = PadPacer()
    private var size = 0 to 0

    private fun now() = SystemClock.uptimeMillis().toULong()

    private fun emit(out: List<PadOut>) = out.forEach { o ->
        Hub.send(BridgeProtocol.PAD) {
            put("msg", JSONObject(o.json))
            put("lossy", o.lossy)
        }
    }

    fun start() = Hub.send(BridgeProtocol.PAD_START) { put("key", key) }

    fun stop() {
        emit(pacer.liftAll(now()))
        Hub.send(BridgeProtocol.PAD_STOP)
    }

    /** The link's path changed: the pacer's timing follows it, and the pad is re-sent whole. */
    fun state(state: String) {
        pacer.setDirect(state == "direct")
        if (state != "off") emit(pacer.resend(now()))
    }

    fun resize(w: Int, h: Int) {
        if (w <= 0 || h <= 0 || size == (w to h)) return
        // Fingers down were in the old frame: they lift before the new size goes.
        emit(pacer.liftAll(now()))
        size = w to h
        emit(pacer.resize(w.coerceAtMost(65535).toUShort(), h.coerceAtMost(65535).toUShort()))
    }

    fun down(id: Long, x: Float, y: Float) =
        emit(pacer.down(id, x.toDouble(), y.toDouble(), size.first.toDouble(), size.second.toDouble(), now()))

    fun move(id: Long, x: Float, y: Float) =
        emit(pacer.moved(id, x.toDouble(), y.toDouble(), size.first.toDouble(), size.second.toDouble(), now()))

    fun up(id: Long) = emit(pacer.up(id, now()))

    fun liftAll() = emit(pacer.liftAll(now()))

    /** Every frame while the pad is up: the relayed tick, repeats, resting re-sends. */
    fun poll() = emit(pacer.poll(now()))
}
