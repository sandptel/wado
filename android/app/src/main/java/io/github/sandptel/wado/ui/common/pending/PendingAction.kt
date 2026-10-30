package io.github.sandptel.wado.ui.common.pending

/**
 * "Every action shows its state" (Decision Log `2026-10-30`), as a pure state machine driven by a
 * clock: pressed at once, working (in-place progress) by 300 ms, "still waiting for <computer>…"
 * with Cancel and Try again past the action's own threshold, then a quiet success or a plain reason
 * with Try again. No frozen button anywhere. Its wire half is the core's op ids and answers.
 */
class PendingAction(private val waitingFor: String, private val longMs: Long = 4000) {
    sealed interface State {
        data object Idle : State
        data object Pressed : State
        data object Working : State
        /** Past the action's threshold: say who is slow, offer Cancel and Try again. */
        data class TakingLong(val waitingFor: String) : State {
            val canCancel get() = true
            val canRetry get() = true
        }
        data class Failed(val why: String) : State
        data object Done : State
    }

    sealed interface Outcome {
        data object Done : Outcome
        data class Failed(val why: String) : Outcome
        data object NoAnswer : Outcome
    }

    var state: State = State.Idle
        private set
    private var since = 0L

    val busy get() = state is State.Pressed || state is State.Working || state is State.TakingLong

    fun start(now: Long) {
        since = now
        state = State.Pressed
    }

    /** Advance by the clock; returns the state. */
    fun tick(now: Long): State {
        if (busy) {
            val t = now - since
            state = when {
                t >= longMs -> State.TakingLong(waitingFor)
                t >= WORKING_MS -> State.Working
                else -> State.Pressed
            }
        }
        return state
    }

    fun answer(o: Outcome) {
        if (!busy) return
        state = when (o) {
            Outcome.Done -> State.Done
            is Outcome.Failed -> State.Failed(o.why)
            Outcome.NoAnswer -> State.Failed("$waitingFor didn't answer. Try again.")
        }
    }

    fun cancel() {
        state = State.Idle
    }

    companion object {
        /** No progress shown before this: most actions answer sooner, and a flash reads as a glitch. */
        const val WORKING_MS = 300L
    }
}
