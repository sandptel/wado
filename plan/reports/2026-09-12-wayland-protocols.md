# Wayland protocols — 2026-09-12

Four protocols added, two closed out with reasons. Full design detail lives in
`memory/compositor/wayland-protocols.md`; this is the state-of-play.

| interface | commit | state |
|---|---|---|
| `xdg_activation_v1` | `4667693` | ✅ **fired live** — Chrome passed a token |
| `wp_presentation` | `7f551e2` | ✅ **fired live** — Chrome collected feedback |
| `wp_single_pixel_buffer_v1` | `7b5c5e3` | ✅ advertised · nothing observable to check |
| `wp_content_type_v1` | `002fc97` | ✅ advertised and logged · ⏳ no app has set a hint yet |
| presentation verdict, both branches | `ecf44b9` | ✅ **positive branch fired live** |
| `wp_cursor_shape_v1` | — | ⛔ skipped permanently |
| `wp_fifo_v1` / `wp_commit_timing_v1` | — | ⏸ deferred on a render-loop blocker |

Smithay's `delegate_dispatch2!(Wado)` already covers all four, so none needed a delegate macro —
only a state field, a global, and (for activation) a `Handler` impl.

---

## `wp_presentation` was parked on a real risk, and the risk was the clock

This is the one protocol here that changes what an app *renders*: GTK and Chrome pace animations
against the timestamps it returns. The reason it stayed deferred was that a wrong timestamp is
worse than none — and the wrong timestamp was four lines from the hook point.

`render_tick` already ends with `window.send_frame(&output, state.start_time.elapsed(), …)`.
That argument is time since daemon start, which is correct for a frame callback (the protocol
takes an arbitrary millisecond counter) and catastrophic for presentation feedback, where the
client compares the value against its own reading of the clock id the global advertised. Copying
that line into the feedback call would have put every frame one daemon-uptime in the past.

The implementation therefore reads `Clock<Monotonic>` on `Wado`, and advertises `clock.id()` as
the global's clock id. **Smithay enforces the pairing silently** — it derives `clk_id` from the
`Time<Kind>` it is handed and *discards* any callback whose id disagrees — so a mismatch is not a
compile error and not a log line. That is what `ecf44b9` exists for.

Two more decisions worth the sentence each:

- **Flags are empty.** `Vsync`, `HwClock` and `HwCompletion` each assert a property of a real
  scanout; `ZeroCopy` would claim the frame reached a display rather than an encoder. Empty is the
  honest encoding of "software composite, timestamped as accurately as this loop can".
- **Collected and answered in one block.** An unanswered callback leaves the client waiting
  forever — the same contract as dmabuf's `ImportNotifier`. Nothing can return early between the
  take and the answer, and `SurfacePresentationFeedback` discards on `Drop` as a second net. The
  encode-failure path reports *not* presented: the composite happened, but nothing reached a
  viewer.

`Refresh::Fixed` derives from the output *mode*, so the rate a client is told and the rate
`wl_output` advertises cannot drift apart. `frame_seq` counts composites (not presentations) and
resets per session — a client that sees a gap after a dropped frame is being told the truth.

## The verdict, on both branches

`environment.md`'s rule from earlier today: *a verdict that only logs "yes" is indistinguishable
from nobody looking.* Presentation needed it more than dmabuf did, because the failure mode here
is a **silent discard** inside smithay.

```
presentation feedback answered — a client is pacing on our timestamps  surfaces=N seq=M
presentation feedback never collected this session — …                 windows=N
```

The negative line carries `windows` for the reason the dmabuf verdict had to learn the hard way:
with no app in the session, nothing could have asked, so `windows=0` reads as vacuous on sight.

`OutputPresentationFeedback` does not expose how much it collected, so the count is taken in the
flags closure by checking `PresentationFeedbackCachedState::callbacks` — scoped so the guard is
released before smithay takes it again to drain the same callbacks.

## `wp_content_type_v1` deliberately does nothing

Invariant 7 fixes the encoder's tuning at session start, so there is no per-surface knob a hint
could turn today. The question *"would the encoder benefit from knowing a video is playing?"*
cannot be answered until we know whether any real app bothers to say — so this advertises the
global and logs the hint, the same pattern as the fractional-scale bind log.

Logged **on change only**; a surface commits at the frame rate. An absent entry and an explicit
`None` are the same non-event.

⚠️ **The one thing to remember if the next session's timing regressed:** this put a
`content_type_log.observe(surface)` call on every `wl_surface` commit, subsurfaces and popups
included. A `with_states` plus a hash lookup, so the cost should be nil — but it is a new call on
the commit hot path, and it shipped in the same daemon swap as `wp_presentation`. It is the cheap
one to rule out first.

## The two that are not coming

**`wp_cursor_shape_v1` — skipped permanently.** It exists so a client can name a cursor instead of
shipping a surface. `SeatHandler::cursor_image` here is an empty body and nothing draws a pointer,
so both halves are no-ops. Implementing it would produce a global whose only effect is to make the
compositor ignore a request slightly earlier.

**`wp_fifo_v1` / `wp_commit_timing_v1` — deferred on a blocker, not on value.** Both are promises
about *when* a commit is latched: fifo says "not until the previous one was presented",
commit-timing says "not before timestamp T". Honouring either means the render tick must be able
to withhold a ready surface from the composite it is already building — a per-surface barrier
inside `render_output`. Today the tick composites whatever `space` holds when the timer fires and
has no concept of "ready but not due". **That barrier is the work; the globals are the easy part.**
Advertising them without it hands an app *wrong* pacing rather than none — the exact trap
`wp_presentation` had to avoid.

## Fired live, 2026-09-12 06:37 UTC — first session on the new daemon

720 × 1614 @ 60, scale 2.5, `bits_per_px=0.0723`. Chrome as the only client.

```
dmabuf path is live … format=AB24 modifier=Unrecognized(144115188348910340) windows=1
activation request — raising and focusing app_id=None
presentation feedback answered — a client is pacing on our timestamps surfaces=1 seq=2382
```

**Both new protocols were exercised by a real client within 300 ms of Chrome mapping its first
surface.** `xdg_activation_v1` is not theoretical: Chrome passes a token on its own startup, which
is the case that used to draw behind.

`seq=2382` is a free sanity check on the frame counter nobody asked for: the session started at
06:37:08 and this line is at 06:37:48, so 2382 composites in ~40 s = **59.6 fps**. The counter
tracks wall clock, the render loop is holding its rung, and no downgrade or stall line appeared
alongside it.

⚠️ **What the positive line does *not* prove.** It counts callbacks *collected*, before
`presented()` runs — so it says a client asked, not that the answer was accepted. A `clk_id`
mismatch would still print it. That branch is unreachable by construction here (both sides derive
from `Monotonic`), but the wording "pacing on our timestamps" is one step stronger than the
evidence. If the clock ever becomes configurable, this line needs to move after the answer.

`wp_content_type_v1` stayed silent — expected with no video playing, and the open question.

---

## Verification, and what it does not cover

- `cargo test -p wado-compositor` — 14 passed. Run with `--all-targets`, because four required
  fields were added to the `Wado` struct literal and `cargo check` alone does not compile test
  targets.
- The check loop was re-validated with a deliberate-error probe in the new module (one type error,
  caught) — the earlier suspiciously-fast `cargo check` runs were cache, not silence.
- `strings target/release/wado` found all four interface names and all four new log messages. The
  running daemon (PID from the swap at 11:20) *is* that binary.
- Relay `{"rooms":0,"servers":1,"status":"ok"}` locally and through the tunnel.

None of that touches whether an app's animation actually looks right. **`wp_presentation` needs one
watched session**: confirm the frame rate holds and `dec` looks normal, and read whichever verdict
line fires. If Chrome stutters after this swap, presentation is the first suspect and the
content-type commit hook is the second.

## What to check by hand

1. **Launch an app from the picker** while another window has focus. The new window should come to
   the front and take focus by itself. Previously it drew behind. `activation request — raising and
   focusing` in the log confirms the path was taken; nothing in the log means the toolkit never
   passed a token.
2. **Start a session, run Chrome, look at the frame rate** in the debug stats. Unchanged from before
   is the pass condition. A regression here is the whole reason this was one daemon swap.
3. **Play a video in the session.** `surface declared a content type` should appear once. If it
   never does, no app in this setup speaks the protocol, and "should the encoder read the hint?"
   answers itself.
