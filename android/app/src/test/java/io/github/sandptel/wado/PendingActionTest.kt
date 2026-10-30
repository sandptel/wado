package io.github.sandptel.wado

import io.github.sandptel.wado.ui.common.pending.PendingAction
import io.github.sandptel.wado.ui.common.pending.PendingAction.Outcome
import io.github.sandptel.wado.ui.common.pending.PendingAction.State
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PendingActionTest {
    @Test fun no_working_state_before_300ms() {
        val a = PendingAction("linux-box")
        a.start(1000)
        assertEquals(State.Pressed, a.tick(1000))
        assertEquals(State.Pressed, a.tick(1299))
        assertEquals(State.Working, a.tick(1300))
    }

    @Test fun taking_long_offers_cancel_and_retry() {
        val a = PendingAction("linux-box", longMs = 4000)
        a.start(0)
        val s = a.tick(4000)
        assertTrue(s is State.TakingLong)
        s as State.TakingLong
        assertEquals("linux-box", s.waitingFor)
        assertTrue(s.canCancel && s.canRetry)
        a.cancel()
        assertEquals(State.Idle, a.state)
    }

    @Test fun no_answer_is_plain_failure() {
        val a = PendingAction("linux-box")
        a.start(0)
        a.answer(Outcome.NoAnswer)
        assertEquals(State.Failed("linux-box didn't answer. Try again."), a.state)
        // A late answer after the outcome changes nothing: one outcome per action.
        a.answer(Outcome.Done)
        assertTrue(a.state is State.Failed)
    }

    @Test fun success_is_quiet() {
        val a = PendingAction("x")
        a.start(0)
        a.answer(Outcome.Done)
        assertEquals(State.Done, a.state)
    }
}
