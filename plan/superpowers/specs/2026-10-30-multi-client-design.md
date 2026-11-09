# Multi-client wado: parallel sessions, co-control and takeover

*Architecture design, 2026-10-30. No product code was changed. It implements Decision Log
`2026-10-30` (1) "Multi-client: parallel sessions with co-control". Every item marked
**USER DECISION** needs your call before the task that depends on it starts.*

---

## 0. The short version

- **Keep the pool.** One session stays one daemon process (crash isolation, one Wayland
  socket per process, one `Output`). Running more sessions means running more daemons.
- **Let several devices reach one daemon.** Do this through the multiplexed *rooms* that the
  light-link work is adding to the relay right now (`?control=1`, the `control` cap, and
  `ctl_open` / `ctl` / `ctl_close` frames). That work sits in worktree
  `.claude/worktrees/agent-a8bf88b8d2ed640f7` and has not been merged or deployed. The relay
  never reads what travels inside a room, so a room can carry a full viewer's WebRTC signalling
  as easily as a widget's. **One relay redeploy covers both features**, as long as the
  control-link relay ships with two small additions (§3 D2).
- **Encode once per session and send the result to every viewer.** webrtc-rs 0.17.2's
  `TrackLocalStaticRTP::write_rtp` already loops over all bindings of one track, so the
  existing single `TrackLocalStaticSample` can be added to N peer connections. The bitrate is
  the lowest any viewer can take. An IDR is coalesced and goes to everyone.
- **Co-control uses one `wl_seat` with an input arbiter.** Each input event is tagged on the
  server with the room it came from, which a client cannot forge. The arbiter remaps touch
  ids, puts short leases on the keyboard and on button-drags, and releases a device's held
  keys and contacts when it leaves. Other participants' pointers appear on each phone as
  client-drawn "ghost cursors"; the compositor still draws no cursor.
- **Resolution:** the session keeps its lead's output. Joiners get a letterboxed picture.
  Input already arrives normalized to the displayed rect, so joiners need no protocol change.
  **Take over** makes you the lead and (USER DECISION) rebuilds the output at your size,
  through the existing `reconfigure_session` fresh-`Output` path. That keeps invariant 8.
- **Take over is decided by the daemon, not the relay.** The daemon makes the taker the lead,
  tells every other participant `session_left{reason:"taken_over", by}`, and closes their peer
  connections. They can **Rejoin** with one tap. They do not retry by themselves, so no I17
  steal-war can start.
- **Light links stay seatless.** They are rooms of kind `control`, and the daemon never lets
  one stream or send input. They show under "devices connected" but never in a session's
  participant list.

---

## 1. The current model, and why it is shaped this way

### 1.1 What exists today

```
 phone A ──/join (seat)──┐                 ┌── /register ── daemon 1 (wado-1.sock) ─ compositor ─ Output ─ encoder ─ 1 PC
                         │   wado-relay     │
 phone B ──/join (seat)──┼─ seats: 1 client ┼── /register ── daemon 2 (wado-2.sock) ─ compositor ─ Output ─ encoder ─ 1 PC
                         │   per instance   │
 widget  ──/join (seat!)─┘   (room.rs)      └── … WADO_INSTANCES (rig), or N × `wado daemon` (instance::claim)
```

| Layer | Today | Where |
|---|---|---|
| Relay | A **seat** pairs exactly one client with one daemon instance (`RoomStore`: "at most one seat per daemon instance"). A seat is parked, held (`hold_ms`, 30 min), gated (`peer_connected` → `peer_accept`), or taken with a tap (`takeover=1` → `taken_over`). Daemon-to-client frames are routed by `instance_id` (`forward_to_client`), so a daemon can address only its one client. | `crates/relay/src/{room,join,register,registry}.rs` |
| Wire | `relay_wire` is frozen. Fields are added only with `#[serde(default)]`, and new behaviour is announced in `caps`. | `crates/protocol/src/relay_wire.rs` |
| Daemon | `RelayCtx` holds **one** viewer: one `active_pc`, `files_pc`, `pad`, `room`, `viewer`, `viewer_ok`, and one e2e `link`, with a `generation` guard so a stale viewer cannot tear down its replacement. `viewer_watchdog` reaps the session after 30 min with no `Connected` PC. | `crates/server/src/relay_client.rs` (2.7 k lines) |
| Compositor | One `Wado` per process: one `Output`, one `wl_seat` named "headless" (keyboard, pointer, touch), one encoder (VA-API DMA → VA-API CPU → x264), and a process-global `WAYLAND_DISPLAY`. Viewer state such as `ViewerAttached`, `ViewerVisible`, `ViewerStrain` and the bitrate is held once per session. | `crates/compositor/src/{lib,state,headless,control}.rs` |
| Pool view | Daemons share `~/.config/wado/{sessions,links,trusted_clients,pending}/` as files, and each one lists every session of the computer. | `crates/server/src/{sessions,links}.rs` |
| Client | One full link per page. `sessions.js` lists the pool's sessions. To reach a session on another daemon it redials with `instance=` and `takeover=1`. `seat.js` handles "Use it here" and "opened on another device". | `crates/client/src/js/{relay_link,seat,sessions}.js` |
| Light links *(in flight)* | `?control=1` opens a seatless room. The relay wraps its frames as `ctl{room_id,text}` on the daemon's one register socket. `server/control/` runs a separate e2e envelope per room and serves only controls. | worktree `agent-a8bf88b8d2ed640f7`: `relay/src/{controls,join_control}.rs`, `server/src/control/` |

### 1.2 Why it is this shape (from the Decision Log)

- **`2026-09-14`: a Remote ID names a pool of processes, not N states in one process.** That
  decision found that N `Wado` states on threads could not work. The process holds one track,
  sets `WAYLAND_DISPLAY` with a process-global `set_var`, and `state.rs` / `headless.rs` are
  written around one `session_active`. Separate processes give crash isolation for free. A
  pool can also grow without a restart (2 → 4 was verified live).
- **`2026-09-14` / I17: nothing takes a room from a live client.** Last-writer-wins
  displacement once produced 132 joins in two minutes. The only move allowed is a takeover
  after a human tap, and the displaced device does not auto-retry.
- **`2026-10-02`: relay finalisation.** The seat outlives both sockets. Policy lives in the
  daemon. The handshake is frozen so the relay can be deployed once.
- **`2026-10-28`: own-seat takeover after a network change.** This is `takeover=1`, sent only
  when the device was joined at the moment the network changed.

### 1.3 What the current model cannot do

1. **Two devices in one session.** A seat is exclusive, so "join" is really "take it".
2. **Address one of several clients on a daemon.** Only the in-flight `ctl` framing makes that
   possible.
3. **Send a joiner its own picture without a second encoder.** Every per-viewer piece of
   state, including the bitrate cap, strain, attached and visible, exists once per session.
4. **Merge input from more than one source.** There is one origin, and touch id 0 from two
   phones would collide.

---

## 2. The target model

**Vocabulary.** These terms are new where marked. "Owner device" already means something in
`gate` (`security { owner }`), so the session-level role is called **lead**.

| Term | Meaning |
|---|---|
| **Computer** | One Remote ID. A pool of daemons on one machine. |
| **Desk** | One daemon process. It holds at most one **session**: one compositor, one `Output`, one encoder, and its apps. |
| **Room** | One device's relay link to one daemon. It is a **seat room** (today's exclusive seat) or a **mux room** (seatless, `ctl`-framed). Mux rooms come in two kinds: `view` (can stream and control) and `control` (widgets only). |
| **Participant** | A device whose room has a live peer connection in a session. |
| **Lead** *(new)* | The participant who started the session or last took it over. The output is sized for the lead's screen. |

```
                        ┌─────────────────────── daemon 2 (desk) ────────────────────────┐
 phone A ─ seat room ─┐ │  ViewerSet{ A(lead), B, C }   one encoder ─▶ one H.264 track     │
 laptop B ─ mux view ─┼─┤      │ origin-tagged input             ├─▶ PC(A)  (binding 1)    │
 tablet C ─ mux view ─┤ │      ▼                                 ├─▶ PC(B)  (binding 2)    │
 widget W ─ mux ctl ──┘ │  compositor: 1 wl_seat + arbiter       └─▶ PC(C)  (binding 3)    │
                        │  W: controls only, never a PC, never input                      │
                        └──────────────────────────────────────────────────────────────────┘
 daemon 1 (desk, idle) ◀─ "Start new session" lands here (relay assignment: idle first)
```

**Flows**

| Action (Computer page) | What happens |
|---|---|
| **Start new** | Same as today: a seat join goes to an idle desk (relay `assign`, idle first). If no desk is idle, see D1 (spawn a sibling, or "N of N in use"). |
| **Rejoin** (my session) | Same as today: the device's own seat, `session_rejoin`. |
| **Join** (someone's session) | A mux `view` room to that desk (`instance=`), then the e2e handshake, then `session_join` and an SDP offer. A PC is added, the shared track is bound, and every participant gets an IDR. The device is now a participant, and the others get a toast. |
| **Take over** | `session_takeover` over a room to that desk. The taker becomes lead, the output is resized (USER DECISION), and every other participant gets `session_left{taken_over, by}`. Their PCs close, and a displaced seat room is set to `seat_hold 0`. |
| **Leave** | `session_detach`. Only this participant leaves, and the session goes on. |
| **End** | `session_stop` (with a confirm that lists who else will be dropped). Who may end a session is a USER DECISION (§3 D6). |

---

## 3. Key decisions

### D1: Process model. How does one computer hold N sessions?

| Option | For | Against |
|---|---|---|
| **P1. Pool of daemons, one session each** (today), plus mux rooms for extra participants | Already built and measured (4 × 1080p@120 in one pool, load 0.95 on 20 cores). Crash isolation. Grows without a restart. No compositor refactor. | Each desk is a whole process (~152 MB idle, measured). "Start new" needs an idle daemon, so the pool size is a ceiling someone has to set. |
| **P2. One daemon running N compositors in-process** | One process, one relay socket. | Already rejected on `2026-09-14`, and nothing has changed: process-global `WAYLAND_DISPLAY`, one `Wado` holding 78 KB of state, a segfault in one session kills all of them. |
| **P3. Supervisor.** A front daemon owns the relay, e2e, gate and WebRTC, and spawns a compositor child per session over IPC. | This is the planned milestone. It spawns on demand with no ceiling, and its crash isolation covers native crashes too. | Big. The typed-channel boundary would have to cross processes (frames as DMA-BUF fds or shared memory). It does not need to land before co-control. |

**Recommendation: P1 now.** Add one small extension, **P1b: sibling spawn.** When a device
asks for a new session and no desk is idle, the busy daemon starts `wado daemon` (the next
`instance::next()`) as a detached sibling with the same environment, then answers
`session_elsewhere{instance}`. A config cap (`server { max-sessions N }`, default 4) bounds it,
and an idle sibling exits after a grace period. That gives spawn-on-demand without the
supervisor. P3 stays the later milestone and replaces P1b when it lands.

> **USER DECISION D1:** Should "Start new" spawn a sibling daemon on demand (P1b, capped), or
> keep a fixed pool and answer "all N desks in use" (today's rig behaviour)? A plain
> `wado daemon` outside the rig appears to run one daemon unless the user starts more, so
> without P1b, **"start your own session" does not work at all** on a default install.
> I have not verified how `wado` / systemd starts daemons in production; check this before
> deciding.

### D2: Relay. How does a second device reach a daemon that already has one?

| Option | Relay change | For | Against |
|---|---|---|---|
| **R1. Virtual seats.** One daemon opens K `/register` sockets (keys `2`, `2.b`, …), and each is one more room. | **None.** Works with the relay deployed today. | Zero redeploy. | The relay's pool numbers lie (`pool_busy`, `pool_size`). Assignment cannot target "a free virtual seat of daemon 2", because `?instance=` names one key. K sockets and K pings per daemon. A new device can be handed a virtual seat on a busy desk. It fights the model. |
| **R2. Generalise the in-flight `ctl` mux rooms.** One registration carries N rooms, wrapped `ctl{room_id,text}`. | **Additive.** It is the same redeploy the light links already need (`control` cap and `CtlOpen`/`Ctl`/`CtlClose`, each `#[serde(default)]`). | One mechanism for widgets and co-viewers. Policy stays in the daemon. Rooms are not exclusive, so the relay has nothing to steal and I17 cannot happen there. | Rooms are not held by the relay, so the daemon must own a participant's absence (it already owns the session's lifetime through `VIEWER_GRACE`). |
| **R3. Relay-managed multi-seat** (session membership and takeover in the relay) | Large, non-additive semantics | n/a | Puts policy back in the relay, which `2026-10-02` explicitly rejected. Needs a redeploy for every policy change. |

**Recommendation: R2.** It has to happen **before the control-link relay is deployed**,
because the relay should be redeployed only once. The control-link work needs two changes and
one documentation change:

1. **Strict targeting.** `join_control::pick()` falls back to "the first daemon that serves
   control" when the wanted instance is missing. That is right for a widget and wrong for a
   co-viewer, who must land on *the desk running the session*. Either:
   (a) add the query field `strict=1` (refuse with `waiting` / "that session's computer is
   reconnecting" rather than fall back), announced as the cap `room_strict`; or
   (b) leave the relay as it is and have the client check `join_accepted.instance_id` against
   the instance it wanted, dropping and retrying if they differ.
   **Recommend (a).** It is a few lines plus a test, and (b) costs a handshake for every miss.
2. **Room kind is the daemon's business.** Nothing in the relay restricts what a `ctl` room
   carries, and nothing should. Only the doc comments ("can never stream",
   `protocol/src/relay.rs:243` in the worktree) should say that this is *daemon policy for
   `control` rooms*. The daemon learns the kind from the sealed first message
   (`room_hello{kind:"view"|"control"}`), never from the relay.
3. **Optional, additive:** `CtlOpen.kind: String #[serde(default)]`, as a hint for logs only.
   Skip it unless the relay logs need it.

**Compatibility matrix.** Every row is safe.

| Relay | Daemon | Client | Behaviour |
|---|---|---|---|
| old (no `control`) | any | new | No mux rooms. Join is hidden; "Use it here" (seat takeover) is today's behaviour. Sessions list works (from files). |
| new | old (no `ctl` cap) | new | `control=1` is refused with `NO_CONTROL`, so behaviour is as today. The client hides Join. |
| new | new | old | The old client uses seats only. The daemon treats its seat room as an ordinary `Viewer` (lead or participant). |
| new | new | new | Full feature. |

The client enables Join only when **all** of these hold: the relay `caps` include `control`
and `room_strict`, the daemon reports `ctl` (via `/online/:id`), and the end-to-end
`E2eHello.v` is at least the multi-viewer version.

**Seats in the target.** Seats stay where they earn their keep: first contact, pairing and
approval (the relay gate), today's held-seat parking on "your desktop is being held", the
network-change own-seat takeover, and old clients. A seat no longer means "exclusive owner of
the session"; the daemon's participant list decides that.

> **Later (not phase 1):** a trusted device's main link could always be a mux room, with seats
> kept only for pairing and old clients. That would remove `takeover=1` / `W.relayKick`
> entirely, because a room has no seat to fight over. It is worth a separate decision once
> rooms have run in production.

### D3: Encoding. One encoder per viewer, or one per session with fan-out?

| Option | GPU cost | For | Against |
|---|---|---|---|
| **E1. One encoder per viewer** | N encodes, plus N scalers if sizes differ | Each viewer gets its own size and bitrate. A slow viewer hurts only itself. | Multiplies the only measured ceiling (concurrent hardware encode). Needs N captures or VPP scales per render. A big compositor change: `state.encoder` is a single `Option`. |
| **E2. One encode per session, sent to N peer connections** | 1 encode, whatever the viewer count | **Almost free structurally.** `ctx.track` is already one `Arc<TrackLocalStaticSample>`, and webrtc-rs 0.17.2 writes every packet to each binding (its own SSRC and PT), collecting errors per binding without aborting (verified in `track_local_static_rtp.rs`). No compositor change. | One bitrate and one keyframe cadence for everyone, so a lossy viewer can drag the others. |
| **E3. E2 plus a second, low layer when needed** ("simulcast-lite") | 2 encodes, only while a slow viewer is present | Isolates a slow viewer. | Needs a second encoder and a VPP scale. Defer. |

**Recommendation: E2**, with these aggregation rules in a new server module
(`viewers/aggregate.rs`). The compositor keeps seeing one viewer.

| Signal | Today (one viewer) | E2 rule |
|---|---|---|
| Bitrate cap (auto bitrate, `2026-10-03`) | the viewer's cap | **min over participants**, with a floor (e.g. 1.5 Mbps). Below the floor, that viewer is labelled "slowed by *device*" and the floor holds. (**USER DECISION D3a**: min-of-all, or lead-first, where the lead's cap wins and others take what they get.) |
| `ForceKeyframe` (PLI/FIR, a viewer connecting) | immediate | **Coalesced**: at most one IDR per 500 ms per session. A join forces one IDR for all, which is unavoidable: the joiner has no reference frame. |
| `ViewerAttached` | the PC is up | **any** participant's PC is `Connected` |
| `ViewerVisible` | page on screen | **any** participant visible |
| `ViewerStrain` (fps shedding) | the viewer's decoder | **any** participant strained. The log and UI name the device. (Could be lead-only under D3a.) |
| Audio (Opus track, `listening`) | one | the same fan-out. `listening` is any. |
| Codec | H.264 by default | **H.264 is forced while more than one participant is present** (invariant 4: H.265/AV1 only if *every* participant has hardware decode, which is not worth negotiating). |

E3 should be added later as a **switch** on Display & stream ("Optimisations as options"),
not as a default.

### D4: Co-control input in Smithay

| Option | For | Against |
|---|---|---|
| **I1. One `wl_seat`, merged input, with an arbiter** | Every app works the same as today. wado's code addresses `state.seat` everywhere (focus, grabs, popups, data device/clipboard, text-input, pointer constraints, relative pointer), and all of it stays as it is. | Participants share one pointer focus and one keyboard focus. Conflicts are possible and need rules (below). |
| **I2. One `wl_seat` per participant** (`SeatState::new_wl_seat` per device) | True multi-pointer and per-device focus. | Every module above becomes per-seat. Clipboard and data-device are per seat, so copy/paste diverges between devices. **Application support for secondary seats is poor and inconsistent (unverified; must be tested per app, Chromium and games especially).** A drag across seats is undefined. Large risk for little visible gain. |
| **I3. One driver at a time** (floor control: input from others is ignored until the driver has been idle for X ms) | No conflicts at all. | That is not co-control. It contradicts the decision ("every device controls it at the same time"). |

**Recommendation: I1**, with `compositor/src/input/arbiter.rs` (one job, one file):

- **Origin tagging at the boundary.** The input channel carries `(Origin, InputEvent)`.
  `Origin` is an opaque `u16` that the server assigns per peer connection, so a client cannot
  claim to be someone else. No network type crosses the boundary. `InputEvent` on the wire is
  unchanged.
- **Touch.** `(origin, id)` is remapped to a unique slot. On origin detach,
  `CancelTouch` / up is sent for every live slot.
- **Keyboard lease.** The origin that pressed the first currently-held key owns the keyboard
  until all its keys are up. Other origins' presses during the lease are **dropped** (and
  logged). This stops A's held Shift from shifting B's typing, because Smithay has one xkb
  state per keyboard. Typical leases last well under a second.
- **Pointer/button lease.** While an origin has a button down or a drag/grab in progress
  (`WindowDrag`, popup grab, `Pinch`), other origins' pointer motion and buttons are dropped.
  Otherwise the last mover wins, and the pointer jumps to it, which is what co-control means.
- **Relative pointer and pointer lock (games).** Merged under the same lease.
  **USER DECISION D4a:** should pointer lock be lead-only, so a second phone cannot yank a
  game's camera? Recommend lead-only while a lock is active.
- **Detach / takeover / PC failure** release everything the origin holds: keys, buttons,
  touches, and a uinput gamepad's buttons if it is in use.
- **Gamepad (uinput mode).** One virtual pad per session (`input/gamepad.rs`), so pads from
  several devices merge. Per-device pads are out of scope. The default key-mapping mode goes
  through the arbiter.
- **Soft keyboard (`text_input` → phone OSK).** The state is broadcast to all, but only the
  **last-input origin** raises its keyboard. Otherwise every participant's phone would pop one.
- **Presence ("ghost cursors").** The server sends each origin's last pointer or touch point
  (normalized), at most 20 Hz, to the *other* participants on an unreliable, unordered data
  channel (`presence`). This keeps invariant 1: presence never rides with input or video. The
  client draws a labelled dot. The compositor still renders no cursor.

### D5: Resolution when co-viewers differ in size (invariant 8)

| Option | For | Against |
|---|---|---|
| **V1. The output belongs to the lead; joiners letterbox** | No output churn. Input is already normalized to the displayed video rect (`protocol/src/lib.rs`: "the client does the letterbox math"), so joiners need nothing new. | A joiner's picture is scaled, so touch is not 1:1 for joiners. Invariant 8 still holds for the lead. |
| **V2. The output follows the active controller** | Everyone gets their own size in turn. | A fresh `Output` and an app reflow every time control moves. That thrashes under co-control, and every IDR changes the SPS. Reject. |
| **V3. One output per participant** in the same session (multi-monitor) | Everyone gets their own size. | Not a shared view: each sees a different region of the space. Not what "join" means. |
| **V4. One render, a per-viewer scaled encode** (E1/E3) | Exact size per viewer. | N encodes. Defer, together with E3. |

**Recommendation: V1.** **Take over** also makes the taker's screen the output size through
the existing `reconfigure_session` (shape change → new `Output` → `retire_output_global` →
`refit_windows`). That keeps invariant 8 exactly.

> **USER DECISION D5:** Should a takeover resize the session to the taker's screen (recommended,
> since "moves a session to you" reads as "fits me"), or keep the current size? Should
> participants get a "Fit to my screen" action that resizes for everyone without a takeover?
> Recommend **not** in v1, because it silently reshapes the lead's screen.

### D6: Session roles, takeover and anti-war rules

- **The lead** is whoever started the session or last took it over, and is shown first in the
  list. If the lead leaves, the role stays with that device (it is a label plus a sizing
  source, not a lock). The unattended reaper (`viewer_watchdog`, 30 min) counts **any**
  participant as attended.
- **Take over** = `session_takeover{instance}`, sent only on a human tap (as today). The
  daemon serialises it on its one loop, so of two simultaneous taps the last one processed
  wins. Every other participant gets `session_left{reason:"taken_over", by}`; its PC closes,
  the arbiter releases its input, and a seat room gets `seat_hold{0}`. The client shows "*Pixel*
  took over this session. [Rejoin] [Home]" and **does not retry**.
- **Cooldown.** A takeover within 10 s of the previous one on the same session is refused with
  "just taken over by *X*, try again in a moment". This stops scripted or accidental
  ping-pong. (I17 cannot happen at the relay anymore, but it could still happen at the daemon
  without this rule.)
- **Rejoin after displacement** is an ordinary **Join**, not a takeover, so it displaces nobody.
- **The same device twice** (two tabs, one `client_key`). The newer room replaces the older
  one *as a participant*. The older one gets `session_left{reason:"opened_here_again"}` and
  does not auto-rejoin. Because nothing retries, there is no war.
- **End session.** **USER DECISION D6:** may any participant end the session, or only the
  lead? Recommend any participant with a confirm that names who will be dropped. It is one
  person's computer, and a participant who cannot end it would have to take it over first.
- **Participant cap per session.** Default 4 (bounded by host uplink, §4).
  **USER DECISION D6b:** confirm the number.

### D7: Where light links (widgets) fit

They are mux rooms of kind `control`, already designed seatless. The co-control work adds
only that `room_hello` names the kind, and that the `server/control/policy.rs` allow-list
stays exactly as it is (no `session_*`, no SDP, no input, no shells, no files). A widget is
never a participant. It is listed under the computer's **connected devices**
(`links/` files), not in a session row. The client rule "dropped while a desktop session
streams" is unchanged.

---

## 4. Performance and resource budget

*Numbers marked "measured" are from `plan/memory/latency/measurements.md` (2026-09-19 pool
run). Everything else is an estimate that a task in §8 has to measure.*

| Resource | Per desk (session) | Per extra participant (E2) | Notes |
|---|---|---|---|
| RAM, idle daemon | **~152 MB (measured)** | about 0 | 4 daemons and 3 sessions used 4.5 GB of 27 GB in total. |
| RAM, one PC | n/a | a few MB (estimate) | DTLS/SRTP, interceptors, jitter state. |
| GPU render | 1 per tick | 0 | One render, whatever the viewer count. |
| Hardware encode (VA-API) | 1 session | **0** | 4 × 1080p@120 did not reach the limit (measured). Mesa/iHD have no fixed session cap. NVENC (not used today) does on GeForce. The real headroom is **not measured**. |
| VRAM for the encode pool at 1080p | roughly 25–50 MB (estimate: 8–16 NV12 surfaces of 3 MB) | 0 | Measure with `intel_gpu_top` / `radeontop` / `nvtop`. |
| Host **uplink** | bitrate B | **+B per participant** | **The real ceiling.** 3 participants × 8 Mbps = 24 Mbps up, which is more than many home uplinks. The min-bitrate rule (D3) plus the participant cap (D6b) bound it. |
| CPU, fan-out | n/a | N sequential `write_rtp` per packet (microseconds, UDP send) | `write_rtp` takes a `Mutex` per call; no new channel hop is added (the pin reason for 0.17 still holds). |
| UDP ports | 100 per slice per daemon | ~5 sockets per PC (measured), ×2 with the files PC | Fine for about 8 participants per desk. Watch I14. |
| IDR on join | n/a | one IDR burst (about 100–300 KB at 1080p, estimate) **to everyone** | Coalesced at 500 ms. Measure glass-to-glass on the existing viewers while someone joins. |
| Relay | n/a | 1 mux room, signalling only | Shares the daemon's 128-deep inbox. SDP and ICE are a few KB per join. The per-IP token bucket (20, one back per 3 s) easily covers main link + widgets + join. |
| Latency for the lead | unchanged | min-bitrate may lower quality, not latency | Shedding fps when any participant strains *does* cost the lead frames, which is why D3a is a decision. |

**Measure before claiming:** (1) concurrent encode headroom, 1080p@60/120, 1 → 6 desks;
(2) the lead's glass-to-glass with 1, 2 and 3 participants (expect it unchanged, and say so if
it is not); (3) IDR cost when someone joins; (4) host uplink saturation with 3 participants on
a home line.

---

## 5. Failure modes

| Failure | Effect | Handling |
|---|---|---|
| A desk crashes (segfault) | Its session and all its participants drop. Other desks are untouched. | Today's behaviour: process isolation. Clients see a `boot_id` change and say "your apps were closed". The `sessions/` file goes stale within 20 s. |
| A participant's PC fails, but its room is still up | It is a ghost participant, and its held input could get stuck. | Per-viewer watchdog: PC not `Connected` for 45 s means the participant is removed, the arbiter releases its input, and the others get a toast. The session is reaped only when **no** participant has been connected for 30 min. |
| A participant's room drops (phone in a lift) | Rooms are not held by the relay. | The daemon keeps the participant entry for `VIEWER_GRACE`. The client redials the room with `strict=1` and rejoins without a takeover. |
| A slow or lossy participant | The min bitrate drags everyone, and PLI storms bring IDR spikes. | A floor plus a "slowed by *device*" label. IDR coalescing. E3 later. |
| Two simultaneous takeover taps | Race | Serialised on the daemon loop; the last wins, and the cooldown refuses a third within 10 s. |
| Takeover ping-pong (two people) | Annoyance, not a protocol war | No auto-retry, the cooldown, and the message names who did it. |
| Two tabs of one device | Duplicate participant | The newer one replaces the older (D6), and the older one does not retry. |
| Strict room targets an absent desk | Join waits | `waiting` "that session's computer is reconnecting", parked on the relay's registry watch. |
| Old relay or old daemon | No Join | Compatibility matrix (D2). The UI hides Join and keeps "Use it here". |
| Network change during a session | Today, own-seat `takeover=1` | Seat rooms: unchanged. Mux rooms: a plain redial, with no takeover needed. |
| Input conflict (two people typing) | Garbled text | Keyboard lease. The losing presses are dropped and logged, never merged mid-chord. |
| No idle desk for "Start new" | Refused | P1b spawns a sibling (capped), or the plain message "all N sessions in use, end one or join one". |
| A sibling spawn fails (GPU busy, port slice exhausted) | No new desk | The busy daemon reports the spawn error verbatim. Never retried silently. |
| A panic in the arbiter | One event lost | It is inside the existing input `catch_unwind`, which drops the event and keeps the session. |

---

## 6. Security: grants per device, per session

- **Each participant carries its own grants, never the lead's.** Each room has its own e2e
  envelope (as `server/control` already does per room), its own proven device key, and its own
  `Grants`. Clipboard sync, shells, files, the trackpad (`input`) and settings are each
  checked against *that* device's line in `trusted_clients`.
- **New grant tokens** (column 6, additive, written by `wado allow|deny`, `wado tui` and
  pairing checklists in `gate/grants.rs`):
  - `join`: may see and co-control a session another device leads.
  - `takeover`: may disconnect a session's other participants.
  - Optionally `view-only`: a join that has its input dropped by the arbiter's origin filter.

  > **USER DECISION S1:** Defaults. Recommend `join` and `takeover` **on** for QR-pinned
  > devices (proven to have seen the host's screen, normally the owner's own devices), and
  > **off** for unpinned or legacy lines. The gate's owner device may do everything, as today.
  >
  > **USER DECISION S2:** Should joining need the lead's consent (a prompt on the lead's
  > screen), or is the `join` grant enough? Recommend the grant is enough, plus a **toast to
  > every participant** and an always-visible participants chip, because a co-viewer sees
  > everything on the screen.
- **Origin is assigned by the server.** Input is tagged with the PC it arrived on, so a client
  cannot impersonate another participant's input or presence.
- **Isolation between rooms.** One participant's sealed frames are never forwarded to another.
  Presence carries only device display names and normalized coordinates. The relay already
  restricts `ctl` delivery to the daemon the room was opened to (`ControlStore::deliver`).
- **What the relay learns.** It can now see how many devices are attached to each desk, as
  room counts. That is metadata only; the content stays sealed (F0).
- **Light links** can never stream or send input. That is daemon policy, and it stays an
  allow-list.
- **The `~/.config/wado` denylist** in the file manager stays non-configurable (unchanged).

---

## 7. Protocol additions (daemon ↔ client, sealed; no relay change beyond D2)

Everything here is a `RelayMsg` inside the e2e envelope. Fields are additive and use
`#[serde(default)]`.

| Message | Direction | Purpose |
|---|---|---|
| `room_hello{kind:"view"\|"control"}` | C→D, first sealed frame on a mux room | Room kind. Decided by the daemon, unknown to the relay. |
| `E2eHello.v` bump | both | Feature gate for multi-viewer. |
| `SessionSummary` + `lead{name}`, `participants[{name, since_ms, link}]`, `max_participants` | D→C | The Computer page's session rows. |
| `session_join{instance}` → `session_started` (the existing negotiation path) | C→D→C | Co-control join. |
| `session_takeover{instance}` → `session_started` / `session_reconfigured` | C→D→C | Become lead (and resize, under D5). |
| `session_left{reason, by}` | D→C | `taken_over`, `opened_here_again`, `ended_by{name}`, `stale`. |
| `participants{list}` | D→C, on change | Live chip and toasts. |
| `session_elsewhere{instance}` / `desks_full{running, max}` | D→C | Answer to "Start new" on a busy desk. |
| `presence{origin_name, x, y}` | D→C, **data channel `presence`**, unordered and unreliable | Ghost cursors. Never on the relay, never in the input channel. |

The input wire is unchanged. `Origin` exists only on the server and compositor side.

---

## 8. Implementation plan

The tasks are as independent as the dependency arrows allow. Each one ends with
`cargo fmt --all --check`, its tests, and a commit. Measurable changes record their numbers in
`plan/memory/`.

```
T0 ─▶ T1 ─────────────────────────────┐
      T2 ─▶ T3 ─┐                     ├─▶ T8 ─▶ T9 ─▶ T11
      T4 ───────┼─▶ T6 ─▶ T7 ─────────┘
      T5 ───────┘        T10 (anytime after T5)
```

| # | Task | Crates / files | Tests |
|---|---|---|---|
| **T0** | Resolve the USER DECISIONs (D1, D3a, D4a, D5, D6, D6b, S1, S2). Write the Decision Log entry, mark the Open Questions, and record that this design extends the in-flight `control` relay. | `WADO_PLAN.md`, `TODO.md` | n/a |
| **T1** | **Relay, before the control-link deploy.** Add `strict=1` targeting (cap `room_strict`) to `join_control`, and correct the doc comments ("cannot stream" is daemon policy). Coordinate with worktree `agent-a8bf88b8d2ed640f7` so it ships in **one** redeploy. | `relay/src/{join_control,signaling}.rs`, `protocol/src/relay_wire.rs` (pin test) | Unit: `pick` with strict never falls back; strict parks while the instance is away. `same_wire_as_relay_msg` still passes. `relay-seat-e2e.mjs`: two mux rooms on one daemon both receive their own frames; strict targeting of an absent daemon parks. `relay-link-check.mjs` unchanged. |
| **T2** | **Server refactor, no behaviour change.** Split `RelayCtx`'s single viewer into `server/src/viewers/{mod,viewer,set}.rs`: one `Viewer` per room (pc, files_pc, pad, e2e link, device, grants, generation), and a `ViewerSet` keyed by room. The seat room is one `Viewer`. `relay_client.rs` becomes a dispatcher. | `server/src/relay_client.rs`, new `viewers/` | Every existing e2e passes unchanged: `session-e2e`, `pool-e2e`, `relay-seat-e2e`, `files-e2e`, `shells-e2e`, `pad-check`, `clip-e2e`. Unit: a stale generation cannot tear down its successor. |
| **T3** | **Fan-out and aggregation.** Bind the shared video and audio tracks into every view room's PC. Add `viewers/aggregate.rs` (min bitrate with floor, any-attached, any-visible, strain rule, coalesced IDR at 500 ms). Force H.264 while more than one participant is present. | `server/src/viewers/aggregate.rs`, the pump | Unit: each aggregation rule as a pure function, plus IDR coalescing with a fake clock. Integration: two PCs bound to one track both decode (new `scripts/cocontrol-e2e.mjs`, two real Chromes, frames counted on both). |
| **T4** | **Input arbiter.** `(Origin, InputEvent)` across the input channel; `compositor/src/input/arbiter.rs`: touch remap, keyboard and button leases, release-on-detach, lead-only lock (D4a). `CompositorCommand::OriginGone(Origin)`. | `compositor/src/{lib.rs,input/arbiter.rs,input/remote.rs}`, `server` input forwarding | Unit (pure, no GPU): two origins with touch id 0 get distinct slots; B's key during A's lease is dropped; detaching A releases keys and buttons; a drag lease blocks B's motion. Existing `touch-check`, `tap-check` and `osk-check` still pass. |
| **T5** | **Protocol.** `room_hello`, the `E2eHello.v` bump, the `SessionSummary` lead and participants fields, `session_join` / `session_takeover` / `session_left` / `participants` / `session_elsewhere` / `desks_full`, and the `presence` channel name. | `protocol/src/{relay,sessions,lib}.rs` | Serde: old JSON (no new fields) still parses. Round-trip every new variant. Old `SessionSummary` files parse. |
| **T6** | **Session membership on the daemon.** Lead tracking, join, takeover with a 10 s cooldown, the same-device replace rule, `seat_hold{0}` for a displaced seat, a per-participant PC watchdog, and the unattended reaper counting any participant. Write participants into `sessions/<instance>.json`. Gate on `join` / `takeover` grants. | `server/src/session/membership.rs` (new, one job), `server/src/sessions.rs` | Unit: a membership state machine table (join, leave, takeover, cooldown, same key twice, lead leaves). e2e in `cocontrol-e2e.mjs`: A starts, B joins (both stream, both type), B takes over (A gets `session_left`, no retry within 30 s), A rejoins (no displacement). |
| **T7** | **"Start new" on a busy desk.** Find an idle sibling (`cli::status::daemons()` plus `sessions/`), or spawn one (P1b, capped) and answer `session_elsewhere`; otherwise `desks_full`. | `server/src/desks.rs` (new) | Unit: choosing an idle sibling, and the cap. e2e on a 1-daemon rig: Start new while busy spawns daemon 2, the client lands there, and the idle sibling exits after its grace. |
| **T8** | **Client: the sessions list and the room link.** `relay_link.js` gains a room mode (`control=1&strict=1&instance=`), with a view `room_hello`. Computer page **Sessions** section: one row per session (apps, size, age, lead and participants, link badges) with Join / Rejoin / Take over / End, "Start new (*k* of *n* free)", a taken-over sheet with Rejoin, toasts, a participants chip in the cockpit, and Join hidden when caps or the version are missing. | `client/src/js/{relay_link,sessions,seat}.js`, `client/src/ui/computer/…` | The existing fixture hook renders every row state (lead only, 3 participants, full, offline, old relay). A Chrome script drives Join, Take over and Rejoin against the rig. `relay-link-check.mjs` still pins the I17 wording. |
| **T9** | **Presence (ghost cursors).** A `presence` data channel (unordered, `maxRetransmits:0`), at most 20 Hz per origin on the server, and labelled dots on the client. | `server/src/viewers/presence.rs`, client JS | Unit: rate limiting, and that a participant never receives its own presence. Chrome script: B's pointer moves and A sees a dot. |
| **T10** | **Grants.** Add `join` and `takeover` (and optionally `view-only`) to `TOKENS`, `wado allow|deny`, the `wado tui` checklist and pairing checklists, with defaults from S1. | `server/src/gate/grants.rs`, `cli/`, `tui/` | Unit: parsing, defaults per pinned or unpinned line, and that a legacy line without the tokens gets the S1 defaults. |
| **T11** | **Measure** (§4 list), and record the results in `plan/memory/latency/measurements.md` and `plan/memory/shared/pool.md`. | n/a | The numbers are the deliverable. Report any that contradict the estimates in §4. |

**Phase 2 (separate decisions, not in this plan):** E3 simulcast-lite as a switch; P3
supervisor replacing P1b; seatless main links for trusted devices; per-device uinput pads;
multi-seat (I2) as an experiment, if app support turns out to be good.

---

## 9. Open items to verify before T0 closes

1. How a production install starts daemons: is it one `wado daemon`, or several? This drives
   D1.
2. Whether the control-link worktree has already been deployed to the Azure relay. If it has,
   T1's `strict` becomes a second redeploy, and D2(b), client-side verification, is the
   fallback.
3. The GPU's real concurrent-encode ceiling (T11 (1)) before setting the D6b cap and the P1b
   `max-sessions` default.

## Decided by the user — 2026-10-30 (overrides the options above)

1. A second device's own session: **start a capped sibling daemon on demand** (cap configurable); it exits when its session ends.
2. Bitrate with several viewers: **lowest viewer wins** (with a floor).
3. Resolution: **the lead's size; take over rebuilds the output at the taker's size** (fresh Output, invariant 8).
4. Pointer lock: **lead only**.
5. Ending a shared session: **any participant** may end it for everyone.
6. Devices per session: **4 by default**, but add a **quick link speed test** (client ↔ host throughput + RTT, and the host's upload/encode/memory headroom) that auto-suggests or fixes the number, **warns** when the experience would degrade, and **refuses creating** a session/join when it would corrupt the experience.
7. New grants `join` and `takeover`: **on for code-paired devices**, off for others until `wado allow`.
8. Joining: **grant + a toast to everyone** in the session; no lead approval.
Also: take over **disconnects** the previous participants with a plain message (they can rejoin); co-control: all participants control at once.

## Implementation notes — F6 (2026-10-30)

What was built against the decisions above, and where it differs from the plan's letter.

| Task | Where | Notes |
|---|---|---|
| T0 | this section | The decisions are recorded here. **The Decision Log entry in `WADO_PLAN.md` (gitignored, main checkout) is still to be written** — not touched from a worktree. |
| T1 | relay (already on main) | `control` rooms + `strict=1` / `room_strict` reused unchanged. No relay change in F6. |
| T2 | `server/src/viewers/` | Not the full `RelayCtx` split. The seat stays in `relay_client.rs`; it and every joined room are participants of one `viewers::Hub` (origin, sealed writer, peer connection). `handle_sdp_offer` moved to `viewers/pc.rs`, shared by both. A joined room is served by `viewers/room.rs` (session verbs, media, controls under the widget policy); shells, files, trackpad and host clipboard history stay on the device's own link. |
| T3 | `viewers/{pc,aggregate}.rs` | One `TrackLocalStaticSample` bound into every participant's PC. Attached/visible = any, strain = any (logged by name), bitrate = lowest participant's cap floored at 1.5 Mbps (a non-lead's reconfigure is read as its bitrate only; the output stays the lead's), IDR coalesced at 500 ms. H.264 is the only codec wado has, so "force H.264" holds trivially. |
| T4 | `compositor/src/input/{arbiter,tagged}.rs` | Input channel carries `InputItem::{Event(origin, ev), Gone(origin), Lead(origin)}` — gone/lead ride the input channel so a release lands after that device's last event. `InputSender::send` keeps the old single-device call sites unchanged (origin 0). Pointer lock lead-only. |
| T5 | `protocol/src/{multi,relay,sessions}.rs` | **Deviation:** no `E2eHello.v` bump (it is in the handshake transcript). A daemon that can be joined says so per session: `SessionSummary.max_participants > 0`; the relay says so with caps `control` + `room_strict`. |
| T6 | `session/membership.rs`, `viewers/mod.rs` | Lead, join (cap), same-device replace, take over with 10 s cooldown, leave, end-by-anyone, 45 s per-participant watchdog for joined rooms, reaper counts any participant, participants in `sessions/<instance>.json`. Ending from *outside* a session needs the `takeover` grant. |
| T7 | `desks.rs` | `desk_request` → `session_elsewhere` (this daemon if idle, an idle sibling, or a spawned `WADO_SIBLING=1` daemon up to `server { max-sessions }`, default 4) or `desks_full`. A sibling exits 30 s after its session ends, or after 120 s unused. Its UDP slice follows its number from the spawning daemon's. |
| T8 | client `ui/computer/{sessions,session_row,linkmeter}.rs`, `js/{multi,linktest}.js`, `multi.rs` | Sessions section on the Computer page; joined rooms via `relay_link.js` room mode; plain-words sheets for take over / ended / refused; toasts for joins. Without the caps, Join is hidden and says why; Take over / End fall back to today's seat takeover. |
| T9 | `viewers/presence.rs`, `js/presence.js` | `wado-presence` data channel, unordered, no retransmits, 20 Hz per origin, never one's own; labelled dots and a "N here" chip. |
| T10 | `gate/grants.rs`, tui, cli | `join`, `takeover` tokens; added by any pairing code (QR or typed); legacy and pre-existing lines get neither until `wado allow`. |
| Speed test (decision 6) | `linktest/{mod,verdict,headroom}.rs` | The device pulls a sealed filler burst over its own relay link and reports down/RTT; the daemon judges with its own headroom (MemAvailable, load per core, encoder, sessions vs cap, best recent upload). Below the floor, RTT > 600 ms, out of memory or out of places → `refuse` (and the join/start it was for is refused); slower than the stream, busy → `warn`; the cap in force is lowered to what the upload carries at the floor. The burst measures the relay path, not the media path — an approximation, labelled as such. |
| T11 | — | **Not measured.** Encode headroom, lead glass-to-glass with 1–3 participants, IDR-on-join cost and uplink saturation remain open. |
