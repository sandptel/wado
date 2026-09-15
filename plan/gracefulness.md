# plan/gracefulness.md — surviving the network

Branch: **`graceful`**. Started `2026-09-13`.

The user's three asks, verbatim:

1. After disconnection the compositor should not die; a reconnect attaches to the session
   that is still running.
2. The client↔relay connection should not stall on every retry or timeout. Keep the relay
   link up, present what is available, then attach.
3. Bitrate and aspect ratio change **on the fly** — the compositor refreshes and continues.

---

## The diagnosis: one lifetime doing three jobs

`session_active` is a single flag covering three things with completely different natural
lifetimes:

| | What it holds | Should live for |
|---|---|---|
| **Desktop** | Wayland display, `space`, `seat`, `app_processes`, window state | hours — until the user says stop |
| **Pipeline** | `renderer`, `encoder`, `capture`, `damage_tracker`, `output`, render timer | as long as the current *shape* (size, fps, bitrate) is right |
| **Viewer** | peer connection, relay socket, strain/shed state | seconds to minutes; it is a phone on mobile data |

Every graceless behaviour is a consequence of collapsing these into one:

- A viewer disconnect runs `stop_session`, which **kills `app_processes`** — so a cell handoff
  costs the user their browser and everything in it.
- Changing bitrate or resolution means stop + start, so it also kills the apps. Hence "on the
  fly" being impossible today.
- The client's relay socket is created *inside* one session attempt's promise, so it cannot
  outlive it, and a failed attempt has nothing to retry on.

**The fix is to give each layer its own verb**, and the pleasant consequence is that one new
verb covers asks 1 and 3 at once: a viewer rejoining at a different resolution and a user
dragging a bitrate slider are *the same operation* — rebuild the pipeline, keep the desktop.

## Every path that currently kills a session

Enumerated before touching anything, because "the compositor should not die" is only as good
as the least-obvious caller.

| # | Path | Verdict |
|---|---|---|
| 1 | **`relay/src/signaling.rs:266`** — relay synthesizes `session_stop` when the viewer's WS closes | **the real killer.** Instant, and it fires on every blip |
| 2 | `viewer_watchdog` → `Stop` after `VIEWER_GRACE` (45 s) | intended, but far too short (I13) |
| 3 | `RelayMsg::SessionStop` from the client (`W.relayStop()`, via `W.stopSession`) | correct — the user asked |
| 4 | `lib.rs:121` `catch_unwind` on a command-handler panic | correct — state is unknown |
| 5 | `lib.rs:158` SIGINT/SIGTERM source | correct, and a **hard ceiling** (see below) |
| 6 | `headless.rs:275` render-timer panic guard | correct |
| 7 | `website/mod.rs` ×3 — the direct HTTP path | out of scope this branch; relay is the field path |

### ⛔ The ceiling this branch cannot lift

Path 5 means **a daemon restart always kills the desktop.** `scripts/rig.sh --daemon` — the
normal deploy step — ends every session. Surviving that needs the compositor to run as a
supervised child process, which is a deliberately deferred milestone in `WADO_PLAN.md`. Not in
scope here; stated so the POC is not mistaken for more than it is.

## Metrics — defined before building, so the POC gets a verdict and not a vibe

| Metric | How | Pass |
|---|---|---|
| **apps survive** | pids in `state.app_processes` before vs. after a forced disconnect | identical |
| **windows survive** | `space.elements().count()` across detach/attach | identical |
| **black-screen ms** | reattach → `ontrack` first frame, client clock | < 1500 ms |
| **loss → picture back** | kill the link N s, restore; measure to first frame | < 5 s + N |
| **idle cost** | daemon CPU% with a session up and no viewer attached | < 5% |
| **spurious timeouts** | client shows a timeout the server did not cause | 0 |
| **reconfigure cost** | bitrate/aspect change → first frame at the new shape | < 1000 ms, apps alive |

---

## Iterations

### Iteration 1 — stop killing the session on a dropped socket

**Finding.** The relay's cleanup path synthesizes `{"type":"session_stop"}` when the client's
WebSocket closes. The comment says why: it was written when the *only* teardown was the WebRTC
peer state reaching `Failed`/`Closed`, which never happens if ICE never completed, so a
timed-out client would leave `session_active` set forever and every later join was refused.

That reason is **obsolete**: `viewer_watchdog` (added later, for a different bug) now covers
exactly that case by two independent clocks. The synthesized stop is left over from the world
before it, and it is the single most destructive line in the codebase for this branch's goal —
it converts *any* socket close into a full teardown with no grace at all, which is why the
45 s grace has never once been observed doing its job.

The comment even names its own successor: *"ponytail: synthesized rather than forwarded; a
PeerDisconnected variant is the clean version."*

**Decision.** Add `RelayMsg::PeerDisconnected` and send that instead. The server logs it,
closes the dead peer connection (freeing its ICE ports — see I14), and leaves the session
running for the watchdog to judge. `SessionStop` goes back to meaning only what a human asked
for.

**Also landed, because the grace period depends on it.** `ViewerAttached(bool)` pauses the
render tick while nobody is receiving. The old 45 s grace was not chosen for the user's
networks — it was chosen because a session with no viewer still rendered and encoded at the
target frame rate for nobody, so leaving one up was expensive. Pausing removes that reason, so
`VIEWER_GRACE` went 45 s → **600 s**.

`ViewerAttached` is also now the single owner of the per-viewer reset (congestion window,
strain flag, shed divisor, keyframe). That had been in two places — `start_session` and the
relay client's rejoin handler — which drifted: a rejoin skipped `start_session`, so a fresh
decoder inherited the divisor the *previous* phone had asked for and was throttled from its
first frame for a fault it never had.

`viewer_attached` starts `true` and is only ever lowered by an explicit command, so the direct
HTTP transport — which never sends it — keeps exactly its old behaviour.

#### Verdict — measured `2026-09-13 01:55`, `scripts/graceful-probe.mjs`

A headless viewer that speaks the relay wire protocol and no WebRTC. It starts a session,
launches a real Wayland client, **yanks its socket with no `session_stop`**, waits, reconnects,
and compares pids.

```
2. launched sleep 9275 — pids [486044,486065]
2. daemon CPU while rendering for nobody: 1.8% of a core
3. socket closed without a session_stop; waiting 5000 ms
3. daemon CPU while detached: 0.0% of a core  (was 1.8%)
4. applications survived — still [486044,486065]
4. the daemon still has the session: hardware
5. rejoined the running session
6. same applications after the rejoin — [486044,486065]
7. an explicit stop still kills everything — no leak
PASS
```

| Metric | Target | Measured |
|---|---|---|
| apps survive a yanked socket | identical pids | ✅ identical |
| session reachable after reconnect | `session_alive` | ✅ |
| pids unchanged across rejoin | identical | ✅ identical |
| idle cost while detached | < 5% | ✅ **0.0%** |
| explicit stop still cleans up | no orphans | ✅ |

Both new log lines fired, so the pass is the new path and not an accident:
`viewer disconnected — session kept, 600s of grace` and `viewer detached — rendering paused`.

⚠️ **Read the CPU numbers honestly.** 1.8% is a *static* kitty window — the damage tracker has
almost nothing to do, so this understates the saving badly. The claim worth making is not
"saves 1.8%", it is **"a paused tick does no work at all, whatever the desktop is doing"**. A
session left with a video playing would have shown the difference properly and was not measured.

⚠️ The probe cannot test the media path. Black-screen-on-reattach and loss→picture-back still
need a browser.

---

### Iteration 2 — the relay link outlives the connection attempt

**Finding.** `relay.js` created the WebSocket *inside* one attempt's promise. The socket, a
single 15 s reject, the join verdict, `session_start` and the whole WebRTC negotiation all
lived in one closure; `ws.onclose` only nulled the handle. Three consequences:

- Every attempt re-dialled from nothing — a new WebSocket, a new TLS handshake through the
  tunnel, a new join — before the viewer could even ask to come back.
- **One timeout covered four different waits.** "handshake stalled" was the answer whether the
  relay was down, no daemon was registered, or the encoder was slow to open. This *is* the
  "stalls on every retry or timeout" the user reported.
- A dropped socket could not be retried, because the thing that would have retried it was the
  promise that had already rejected.

**Decision.** Split by lifetime, which is also what `CLAUDE.md` asks for when a file does two
jobs.

| File | Job |
|---|---|
| `js/relay_link.js` (new) | keep one socket to the relay open. Dial at page load from the saved settings, reconnect forever with backoff (0.5 s → 15 s cap), dispatch by message type. Synthetic `__up` / `__down` types so sessions can react to the link without owning it. |
| `js/relay.js` (rewritten) | session verbs over that link. No attempts, no connection promise. |

Consequences worth naming:

- **Pressing Start on a warm link is one message down an open socket.** No dial, no join, no
  timeout — which is exactly what the user asked for: *"why don't we stay connected to the
  relay whenever we can beforehand and just create the webrtc connection?"*
- **A reconnect mid-session rejoins without prompting.** The viewer never chose to leave.
  `session_error` on that path clears `sessionOn` and starts fresh, so a client whose session
  *did* expire cannot renegotiate forever against something that is gone.
- **`join_denied` is retried, not fatal.** It usually means a daemon that is restarting; it used
  to be a dead end the user had to press Start to leave.
- **The one remaining timeout is per request** — "I pressed Start, is a session coming?" — armed
  only while a human is blocked on it.
- `W.MAX_RECONNECTS` 10 → **30**. It was sized to fit inside a 45 s grace; at 600 s, giving up
  after ~37 s was the last place a viewer was told "connection lost" for a network that came
  back.

**No protocol change.** `SessionStart` already answers `SessionAlive` when one is running and
`SessionStarted` when it is not — it *is* the query. A `SessionQuery` variant was designed and
then dropped: a new protocol variant, a relay pass-through and a server arm, to learn what one
existing message already says.

#### Verdict — `scripts/relay-link-check.mjs`, 24 cases, all pass

Both files loaded into one scope, the way the bridge concatenates them, driven by a fake
WebSocket that records *which* socket each send landed on — the question a captured-`ws`
closure gets wrong and that the old structure could not even be asked.

The cases that matter are the ones the old code could not express: a socket closing mid-request
(rejects, does not hang), a reconnect with a session running (**exactly one** rejoin — not zero,
not one per attempt), a handler firing after a reconnect (writes to the new socket, not the
dead one), and the drop-and-restart flag cleared when the link goes down under it.

**It earned its keep before running a case:** loading the two files in bundle order was a
`TypeError` — `relay.js` calls `W.relayOn` at load and `relay_link.js` defines it, so the link
has to come first in `bridge.rs`. That would have been a blank page on the phone.

`scripts/reconnect-check.mjs` now reads `MAX_RECONNECTS` from `core.js` **and** `VIEWER_GRACE`
from `relay_client.rs`, and asserts the budget fits inside the grace. The two had already
drifted once; a test that reads both cannot let them drift again.

⚠️ **This iteration is not measured to iteration 1's standard.** The probe cannot reach any of
it — it is all browser-side. What exists is the harness plus a human on the phone.

---

### Iteration 3 — change a running session's shape

**Decision.** `reconfigure_session` rebuilds only what depends on the numbers that changed:

| rebuilt | why |
|---|---|
| encoder + capture target | both are allocated at a fixed resolution |
| `Output` | **invariant #8** — Wayland cannot un-advertise a mode, so a resize is a fresh output, never a mutated one |
| damage tracker | it is built *from* an output |
| render timer | the tick interval is baked into the timer's closure |

**Not** rebuilt: the `GlesRenderer` and the dmabuf global. The global's format list comes from
the renderer and the renderer does not care what size we draw, so tearing them down would hand
`failed()` to every client holding a dmabuf, for nothing.

Wired as `SessionReconfigure` → `SessionReconfigured`, deliberately *not* `SessionStarted`:
that message is what tells a client to negotiate WebRTC, and a reconfigure must not. The track
is the same one; the decoder picks up the new size from the forced IDR. Renegotiating would cost
a fresh ICE round and a black screen to change a number.

The client half is a single **Apply** button next to Start/Stop.

#### Verdict — `scripts/graceful-probe.mjs`

```
7. bitrate only in 47 ms — 60 fps, 2500 kbps, hardware
7. bitrate only: applications survived
7. resize 1280x720 -> 960x540@30 in 7 ms — 30 fps, 1800 kbps, hardware
7. resize: applications survived                       (Chrome, 12 processes)
7. odd width 1281: refused — "width 1281 is out of range (160-7680, even)"
7. zero fps: refused — "fps 0 is out of range (1-240)"
7. absurd scale: refused — "scale 99 is out of range (0.5-4.0)"
7. 8K: accepted by this hardware
7. recovery after the extremes in 7 ms
```

**6–50 ms**, against a 1000 ms budget, with the applications alive. One caveat recorded as
**I16**: `kitty` exits when the output is replaced. Chrome — the real use case — does not.

---

## Hardening pass

Asked for after the field confirmation. Each item has a check that fails without the fix.

### The diagnostic gap that cost a cycle

`ClientData::disconnected` was **an empty function**. A Wayland client dying produced no log
line at all — not for an ordinary exit, not for a compositor-inflicted protocol error. When a
session's application vanished two seconds after a reconfigure, nothing anywhere could say
whether it had exited on its own, been killed, or been disconnected by us. Those are three
different bugs with three different fixes.

It now says, and names a `ProtocolError` as **the compositor's fault, not the application's** —
because libwayland disconnects a client that touches a dead object, and a client whose display
dies exits, so our mistake surfaces as an application mysteriously quitting several layers away.

That line is what turned I16 from a guess into a measurement: `ConnectionClosed`, not
`ProtocolError`. We were not killing it.

### Five fixes

| # | Was | Now |
|---|---|---|
| A | a malformed relay message was logged and dropped | answered with `SessionError`. A dropped message leaves the sender waiting out a timeout unable to tell rejection from a wedged daemon — which is exactly how the probe's own wrong `Quality` shape presented |
| C | `SessionConfig` went from an untrusted socket to `Output::new` and the encoder unchecked | `SessionConfig::validate` **on the type**, used by both session verbs. Odd dimensions have no valid 4:2:0 chroma plane; `fps: 0` is a zero-nanosecond timer interval, i.e. a render loop that never yields to input |
| D | `render_tick` unwrapped renderer/capture/tracker/encoder | skips the frame and keeps the session. **Any** half-built pipeline was a panic whose recovery is `stop_session` — turning a recoverable encoder error into a destroyed desktop |
| F | the link's backoff reset on `join_accepted` | resets only after the link has held 5 s. A relay that accepts and immediately closes was retried every 500 ms forever. Retries stay unlimited; only the *speed* is earned |
| H | running out of WebRTC retries called `stopSession` | relay mode stops retrying, not the session |

**H is the one that mattered most.** `giveup` sent `session_stop`, so a client that exhausted
its retry budget **destroyed a session the daemon was holding for another eight minutes** — the
exact inverse of this branch's purpose, firing in precisely the case that motivated it: a dead
zone longer than the budget. Recovery now belongs to the link, which is still up and
reconnecting. Direct mode has no link and no server-side grace, so it keeps the old behaviour.

### Deliberately not done

- **Room-id tracking on `PeerDisconnected`.** The relay's `Room::create` fails while a room
  exists for that Remote ID, so a stale disconnect cannot arrive after a new viewer's connect.
  Guarding an unreachable path is speculative; this note is the guard.
- **A rollback path in `reconfigure_session`.** Fix D makes a failed reconfigure survivable
  without one. ⚠️ **Untested against a real failure**: 8K was *accepted* by this hardware, so
  nothing could be found that fails the pipeline build deterministically. D is reasoned, not
  measured — labelled as such per `CLAUDE.md`.
- **Viewer arbitration** — recorded as **I17**: two tabs both auto-rejoining now fight over the
  peer connection indefinitely.

## Metrics — final

| Metric | Target | Measured |
|---|---|---|
| apps survive a yanked socket | identical | ✅ |
| session reachable after reconnect | `session_alive` | ✅ |
| idle cost while detached | < 5% | ✅ **0.0%** |
| reconnect → streaming, in the field | — | ✅ **8 s**, window intact (02:14:35→02:14:44) |
| reconfigure cost | < 1000 ms | ✅ **6–50 ms** |
| apps survive a reconfigure | identical | ✅ Chrome; ❌ kitty (I16) |
| invalid config | refused, session intact | ✅ |
| spurious timeouts | 0 | ⚠️ not yet observed in the field over a long session |

---

## Coda — the number that was there all along

The server has published `SentKbps` since the first commit on this branch. **Nothing read it**,
so the verdict kept guessing, and kept guessing the same way:

| when | sent | arrived | `packetsLost` | verdict |
|---|---|---|---|---|
| 2026-09-12 22:33 | 5.35 Mbps | 2.44 Mbps | 0 | `bad the server` |
| 2026-09-13 01:19 | 5.13 Mbps | 524 kbps | 0 | `bad the server` |
| 2026-09-13 12:04 | — | 91 kbps | 0 | `bad the server` |

Render pacing held 90/90 and the pump was clean through all three.

Now three answers instead of one: sent-and-not-arrived blames the path, not-sent quotes the
daemon's own figure, and no report says so rather than sounding as certain as the other two.

**Two bugs the new cases caught, both in work written the same hour:**

- the harness set the sent figure *before* `setTargetKbps`, which correctly clears it — so two
  cases asserted against a value that had been wiped, and passed for the wrong reason;
- **`+null` is `0`.** An absent reading was stored as a measured zero — which is not "no
  evidence", it is the strongest possible accusation against the server, quoted as fact.

### The shape this run kept producing

Three separate times, the thing blocking a diagnosis was **a measurement that existed and was
not consulted**:

| | |
|---|---|
| `ClientData::disconnected` | an empty handler — a client dying said nothing at all |
| `SentKbps` | published on the wire for hours, read by nobody |
| `document.visibilityState` | known to the client, absent from the stats line (I18) |

None of them were hard to add. Each one cost a debugging cycle by being absent, and in two cases
the absence did not read as "unknown" — it read as a confident wrong answer. **A number you
collect and never consult is a number you do not have**, and an absent value coerced into a
default is worse than a gap, because a gap is visible.

---

## Field results — `2026-09-13`, the user's own phone on mobile data

Everything below is measured on the live rig, not on the probe.

| claim | measured |
|---|---|
| session survives a disconnect | **7 min 54 s** detached, window intact (07:18:16 → 07:26:10) |
| reconnect → streaming, unattended | **~1 s**; five recoveries in eighteen minutes, gaps of 33 s, 47 s, 74 s and **246 s** |
| idle cost while detached | **0.0%** of a core |
| hidden page costs nothing | **11 432 → 0 kbps** in one sample window, same-millisecond log line |
| reconfigure | **6–50 ms**, applications alive |
| spurious sheds after the I19 fix | **zero**, against one every couple of minutes before |

The single line that says what the branch was for:

```
07:47:22.091  client joined — room created
07:47:22.503  browser: the session survived the outage — rejoining
07:47:23.068  browser: peer state: connected
```

Under a second, nothing pressed. Before this branch every one of those gaps destroyed the
session outright — the relay synthesized `session_stop` the instant the socket closed, and the
246 s gap would have outlived the old 45 s grace anyway.

## What this run kept teaching

**Three regressions came from two individually-correct changes composing.** Not one of them was
visible in any single file, and all three were found in the field:

| | the two right things | what they made |
|---|---|---|
| I17 | retry `join_denied` + auto-rejoin on reconnect | two devices trading the session every 18 s |
| I19 | reset per-viewer state on attach + attach on every reconnect | re-flooding the phone at full rate every reconnect |
| skew | answer unknown messages + send a new message | a new client tearing down its UI against an old daemon |

**Four measurements existed and were not consulted**, and in three cases the absence did not read
as *unknown* — it read as a confident wrong answer:

| | |
|---|---|
| `ClientData::disconnected` | an empty handler; a dying client said nothing at all |
| `SentKbps` | published on the wire for hours, read by nobody |
| `document.visibilityState` | known to the client, absent from the stats line |
| `refreshHz` | measured since long before it mattered, never left the browser |

Adding the third disproved the theory it was added for (I18) and immediately exposed a larger
problem it was not built for (I20). Adding the fourth needed **no protocol at all** — the client
knew both numbers and `rlog` already reached the daemon log. *A measurement does not need a
protocol just because the answer is wanted server-side.*
