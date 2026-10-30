package io.github.sandptel.wado.core

import android.content.Context
import android.os.Build
import android.os.SystemClock
import android.util.Log
import io.github.sandptel.wado.bridge.Computer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import uniffi.wado_mobile_core.CoreEvent
import uniffi.wado_mobile_core.WadoCore

/**
 * The Rust client core in this process: made off the main thread at start, its events pumped on
 * `Dispatchers.Default` into [model] (home) and [events] (anything else that listens). The core
 * never calls into the JVM: Kotlin pulls (`nextEvent`, a suspend fun).
 */
class CoreHost(private val context: Context) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val _model = MutableStateFlow(HomeModel())
    val model: StateFlow<HomeModel> = _model
    private val _events = MutableSharedFlow<CoreEvent>(extraBufferCapacity = 64)
    val events: SharedFlow<CoreEvent> = _events
    private val _ready = MutableStateFlow<WadoCore?>(null)
    /** The core once it is up (null for the first moments of a cold start, or if it failed). */
    val core: StateFlow<WadoCore?> = _ready
    /** Why the core could not start, for Diagnostics. */
    @Volatile var failure: String? = null
        private set

    /** Process start → the first `Link{open}`, ms: the "live status" budget (A8), logged once. */
    @Volatile var firstOpenMs: Long? = null
        private set

    /** Start the core; `legacy` is what the web part knows (merged into home by Remote ID). */
    fun start(legacy: Flow<List<Computer>>) {
        scope.launch { legacy.collect { list -> _model.update { CoreState.legacy(it, list) } } }
        scope.launch {
            val c = runCatching {
                WadoCore(context.filesDir.resolve("core").absolutePath, KeystoreSecretBox(), deviceName())
            }.onFailure {
                failure = it.message ?: it.javaClass.simpleName
                Log.e(TAG, "core did not start: $failure")
            }.getOrNull() ?: return@launch
            _model.update { m -> CoreState.reduce(m, CoreEvent.Computers(c.computers())) }
            _ready.value = c
            while (true) {
                val ev = c.nextEvent()
                if (firstOpenMs == null && ev is CoreEvent.Link && ev.link.kind == "open") {
                    val ms = SystemClock.elapsedRealtime() - android.os.Process.getStartElapsedRealtime()
                    firstOpenMs = ms
                    Log.i(TAG, "budget: cold start → live link words in $ms ms")
                }
                _model.update { CoreState.reduce(it, ev) }
                _events.tryEmit(ev)
            }
        }
    }

    /** The pairing screen closed: forget its last state. */
    fun clearPairing() = _model.update { it.copy(pairing = null, justPaired = null) }

    fun watch(keys: List<String>) {
        _ready.value?.watch(keys)
    }

    fun pair(link: String): Long? = _ready.value?.pair(link)?.toLong()

    fun pairTyped(relay: String, remoteId: String, code: String): Long? =
        _ready.value?.pairTyped(relay, remoteId, code)?.toLong()

    fun forget(key: String): Long? = _ready.value?.forget(key)?.toLong()

    private fun deviceName(): String {
        val model = Build.MODEL.orEmpty().ifBlank { "Android" }
        return "$model · wado app"
    }

    private companion object {
        const val TAG = "wado-core"
    }
}
