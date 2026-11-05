# ▣ 2026-11-04 evening — the user's reports on app 0.4.0 (fix one by one, in this order)

| # | Report | State |
|---|---|---|
| L1 | **The whole app lags** — every back gesture and scroll drops to ~1–2 fps, worst in the menus | `90d3269` `ee1d30f` — reasoned, not measured: page layers, SVG loaders kept, narrow listeners. Needs the phone's Diagnostics `slow frame` lines if still slow |
| L2 | **Pairing refused while another device has a session** ("872-990-894 is busy with another device's desktop") — parallel use: only a resource that cannot be shared may block | fixed `0c0bf25` (app) `068b714` (web) — control-room pairing, then the seat; verified 2026-11-05 on the live daemon: a headless browser paired by QR while another device held a session, log `control link: opened (no seat…)` → `paired` → seat; test device unpaired |
| L3 | **`wado pair` prompt is inhuman** | fixed `0c0bf25` — one screen, defaults ticked, arrows/space/←→, Enter |
| L4 | **QR scanning in the app does not work at all** | fixed `fc71740` (Google code scanner, zxing fallback) — needs a phone check |
| L5 | **A QR opened from Google Lens goes to the browser, not the app** | fixed `fc71740` (page's `intent:` handoff; `deploy/assetlinks.json` for verified links) — needs a phone check; verified links need the file at `sandptel.github.io/.well-known/` |
| L6 | **Virtual input says "Offline"** while connected (another session / files) and the doors are dead | fixed `fc71740` (Reach, not the seat link; a shut door explains) |
| L7 | **The app is called "wado next"** — it is just wado | fixed `fc71740` (label, About, Diagnostics, APK name; applicationId kept so pairings survive) |
| L8 | **Home bar's scan and ⋯ float loose** ("a complete miss") | fixed `fc71740` (one plate with the chip) |
| L9 | Web: complete end to end (Game mode full screen + pad, top bar, Continue, tips, tour, devices QR, tablet/desktop fit), then a design debug pass | `968dfa0` `1ba56ea` `491348b` `3cd91f8` — top bar + play grid, lean apps, readouts, dock keyboard, 48 px, Game mode sheet was invisible (class clash with the widget picker), shelf row/grid by pointer, computer page centred offline, Devices QR. Screenshots phone/tablet/desktop via CDP (`scratchpad/cdp.mjs`). Not shot: a live session's console (needs a paired browser in a session) |
| L10 | **App drawer never gets the apps** ("Asking bushido for its apps…" forever); the keyboard covers the drawer, no visible typing box | fixed `90d3269` — the ~1 MB list (icons inline) held to the relay's 64 KB/s lane, re-asked at 30 s; now lean, icons 24 at a time; the drawer lifts above the keyboard. The text-wanted path's missing type box → open |
| L11 | **Control centre flashes constantly in a session** | `ee1d30f` — reasoned: a fresh SVG loader per rebuild (stats tick); one loader each now. Phone check |
| L12 | **Apply does nothing** on Display & session | fixed `90d3269` — it applied (daemon log); the bar never listened to an apply, and counted auto bitrate's cap |
| L13 | **Console wastes space** outside full screen; readouts in the worst place; session/room ratio uncomfortable; small targets; quick actions | app `20e4e71` — refit on deck toggle, readouts at the bottom edge, health only when wrong, fps/ping opt-in, 48 px, keyboard in the dock. Web: 48 px + dock keyboard (`1ba56ea`); a room-sized session measured unnecessary — the web console's picture box already has the screen's aspect (390×844 → 270×584) |

# live-issues — reliability TODO from live testing (2026-10-30)

Owner: live reliability investigator (worktree `agent-af3c51c662959d0b3`). Lives in the worktree's
gitignored `plan/` because the isolation guard refuses writes to the main checkout.
Daemon: `/home/bushido/Projects/wado/wado/target/release/wado daemon`, pid 1542138 (started 10:00:52Z),
log `~/.cache/wado/daemon.log` (ANSI-coloured; strip with `sed 's/\x1b\[[0-9;]*m//g'`).
Client: Android · Chrome on cellular (157.49.x, 464XLAT `192.0.0.4`), relay Azure indiasouthcentral
(no `lane` cap deployed: relay caps are park,hold,takeover,gate,ping,leave,seat_hold,occupancy).

Status values: `todo` · `investigating` · `cause found` · `fixed in <commit>` · `can't reproduce`.

| # | first seen (UTC) | user would see | stage | status |
|---|---|---|---|---|
| L1 | 10:19:36 | a touch/scroll gesture ignored (10 events in 60 ms) | client builds input with a `null` (NaN) coordinate → daemon serde rejects | investigating |
| L2 | 10:20:50, 10:20:54 | "Lutris" launched twice, nothing appears | app launch: `Error: unable to open display` | investigating |
| L3 | 10:06:23 | passkey unlock challenge, never answered; device left 5 s later | passkey (other agent's area) | todo — report |
| L4 | 10:20:37 → | picture ~127–148 ms behind (jbuf 9 → 142 ms) after Chrome launched + autorate 5043→2522 | browser jitter buffer | investigating |
| L5 | all session | pad arrivals `late` up to 211 / `starved` up to 74 per 5 s | pad over WebRTC on cellular (virtual-input agent's area) | todo — record |
| L6 | 10:02–10:19 | pad tears down and rebuilds its WebRTC link on every rotation (`pad: closed` → `pad: open` ≈1 s) | pad link lifecycle (virtual-input agent's area) | todo — record |
| L7 | 10:06:13, 10:19:27 | client log panel misses lines | lane: `100/115 log lines not sent — the link was busy` | todo |
| L8 | 10:06:00 | Spotify seek does nothing | host action: `this player cannot seek` | todo (player refusal; UI should hide seek?) |
| L9 | all session | none | 556× `webrtc_sctp … unable to parse SCTP packet chunk too short` WARN in 20 min | todo (noise) |
| S1 | structural | any request "the computer didn't answer" while another is slow | daemon relay loop handles every frame in sequence and `.await`s handlers inline | investigating |
| L10 | 10:21:44–10:22:13 | **stuck wait: no picture for 30 s after a blip**, then Stop | `session_rejoin` ×2 answered `session_started` by the daemon; client never logged `session ready`, never sent an offer | investigating |
| L11 | 10:22:07, 10:22:18 | speed test "no answer" ×2 (shows as old daemon) | `link_test` → no `link_test_done`/`link_verdict` within 8 s | investigating |
| L12 | 10:20:50–10:21:03 | Lutris tapped 3×, nothing ever appears, no message | Lutris crashes: pygobject `TypeError: Unsupported type: <GType invalid (0)>` (+ `glxinfo` no DISPLAY); launcher said nothing | cause found (app bug); wado now logs it (dc19d01) |
| T1 | coordinator ask | close/start/launch/pad "no feedback" | timings below | measured |

## T1 — daemon-side timings of user actions (coordinator request)

| action | request → done on the daemon | evidence |
|---|---|---|
| session stop | **94 / 82 / 9 ms** (`session_stop received` → `compositor session stopped — resources released`) | 18:44:49, 06:08:45, 10:22:13 |
| session start | 87 ms to `compositor session active`; offer→ICE connected 475 ms; first decode at the browser ≈ 3.0 s after the start | 10:19:27.50 → .59; 29.27 → 29.75; decoder 30.50 |
| session start, gap | ≈1.4 s between `session active` (27.59) and the client's `session ready` (29.04), with `lane: 115 log lines not sent — the link was busy` at 27.71 | the outbound lane was busy right then — what with is unknown (new lane timing will say) |
| app launch (Chrome) | 1.15 s / 1.46 s launch → first buffer — the app's own start | 10:19:56.19 → 57.35; 06:07:27.46 → 28.92 |
| pad Run → Ran | **not measurable**: daemon logs `pad: ran` only after; client `pad_actions.js` 8 s give-up has no rlog | virtual-input agent's files |

Acks on the wire: `session_launch`→`session_launched` (immediate — means "queued", not "shown"),
`session_stop`→`session_stopped`, pad `run`→`ran` (by id), `host_do`→`host_state` after a fixed
250 ms sleep. **No ack exists for "the window appeared"**; dc19d01 logs it daemon-side only.

## L10 — rejoin answered, client never streams

- **time**: 10:21:36 page hidden → 10:21:37 relay link closed → 10:21:39 new room ace85b75, sealed →
  10:21:44.63 `viewer rejoined the running session` (daemon sent `session_started`) → 10:21:51.73
  again → no `browser: session ready`, no `sending offer`, no `browser:` line at all until
  10:22:07 → 10:22:13 `session_stop`
- **user**: picture never came back; pressed Stop after ~30 s
- **stage**: daemon → device direction. Device → daemon frames arrived (rejoin ×2, rlog, stop);
  daemon → device answers (`session_started`, later `link_test_*`) apparently did not
- **hypothesis A**: outbound frames stuck behind a big frame on the relay socket (the daemon's
  outbound lane was "busy" at 10:21:29; urgent frames are charged to the pacer and a 1.2 MB
  `apps_list` or a 530 KB speed test is one TCP-serialised blob on relay→phone)
- **hypothesis B**: client-side — the page (hidden at 10:21:36) did not process them
- **evidence needed**: lane `slow:` lines (31b2b3c), ask clock `still unanswered` (fa815de)
- **status**: investigating

## L11 — speed test "no answer"

- **time**: 10:22:07.33, 10:22:18.17 (client rlogs); no daemon line for `link_test` either way
- **code**: `linktest::send` queues 24 × 16 KB chunks (urgent class, ~530 KB sealed) then
  `link_test_done`; the client waits 8 s for `link_verdict`. On a joined room (`viewers/room.rs`)
  the frames go through the control writer, which paces — 530 KB at the lane rate is ≥ 8 s,
  i.e. exactly the client's limit (hypothesis for the room path; this one was the seat link)
- **also**: no `browser: speed test: N kbps` line appears anywhere in today's log — the test has not
  completed once on this phone today (10:19:27's judged figures are `down_kbps=0 rtt_ms=0`, the
  fallback judge, not a measurement)
- **status**: investigating

---

## S1 — the relay main loop is one sequential `.await` chain

- **evidence (code)**: `relay_client.rs` main loop (`// ── 4. Main message loop`) reads a frame, unseals,
  then runs the match arm inline. Arms that await before the next frame is read: `HostGet`/`HostDo`
  (D-Bus: `host::state()`, `host::act()`, +250 ms sleep), `AppsRequest` (`apps::running::mark`),
  `TargetsRequest` (a11y), `MenuActivate` (a11y + 200 ms sleep), `PadOffer` (`pad.open` → WebRTC pc),
  `FilesOffer`, `FilesIn` (`files_lane.request`), `SessionStart`/`Reconfigure` (up to 5 s
  `reply_rx` timeouts), `SdpOffer` (ICE gather), `DeskRequest`, passkey/claude/push handlers,
  `pc.close().await` on disconnect.
- **consequence (hypothesis)**: one slow D-Bus call or ICE gather delays every frame behind it —
  including `pad_in` input on the relay transport and the answers other pages wait on.
- **evidence needed**: per-frame dispatch time. Instrumentation added: `slow:` WARN when one frame's
  handler holds the loop ≥ 250 ms, with kind + seq.
- **status**: investigating

## L1 — `bad input event: invalid type: null, expected f64`

- **time**: 10:19:36.214–.277Z, 10 lines, 60 ms (session d275ab9a, 7 s after the stream started)
- **user**: one gesture dropped on the floor
- **log**: `WARN wado::viewers::pc: relay client: bad input event: invalid type: null, expected f64`
- **correlation**: none — the warning does not say which event type or which field
- **stage**: daemon decode of a WebRTC input-channel message (`viewers/pc.rs`); JSON `null` is what
  `JSON.stringify` writes for `NaN`/`Infinity`, so a client coordinate was NaN
- **hypothesis**: a coordinate mapped against a zero-size rect, or a velocity divided by a zero dt
- **evidence needed**: the raw event. Instrumentation: the warning now carries the raw JSON (truncated)
- **status**: investigating

## L2 — `Error: unable to open display` (Lutris)

- **time**: 10:20:50.733Z (pid 1718020), 10:20:54.316Z (pid 1719354)
- **user**: tapped Lutris twice, no window
- **log**: `launched session application … command="lutris"` → `wayland client connected` → `Error: unable to open display`
- **stage**: app start inside the session
- **hypothesis**: Lutris (or something it starts) needs X11 / `DISPLAY`; no Xwayland in the session. The
  launcher logs "launched" and nothing when the app dies
- **evidence needed**: session env (`DISPLAY`), lutris exit status
- **status**: investigating

## L3 — passkey unlock challenge with no answer

- **time**: 10:06:23.035Z challenge sent (purpose=unlock), 10:06:28 viewer disconnected; again 10:06:39 → left 10:06:52
- **user**: passkey sheet, then the page left
- **stage**: passkey (owned by the passkey agent, already fixing a stuck authenticator)
- **status**: todo — reported to coordinator, not edited

## L4 — playout delay grows to ~140 ms

- **time**: 10:19:29 jbuf 9 ms → 10:20:29 autorate 5043→2522 kbps → 10:20:49 jbuf 142 / jtarget 148
- **user**: verdict "the picture is about 127 ms behind"; latency spike
- **log**: `browser: stats … jbuf=142ms … jtarget=148ms … abuf=144ms atarget=140ms`; daemon side clean:
  pump queue 0 ms, write_sample p99 0.2 ms, render 60 fps, audio spacing p90 10.7 ms
- **stage**: browser jitter buffer. atarget jumped 60 → 140 ms at 10:20:29 and jtarget followed
- **hypothesis**: the audio jitter target drags video playout with it (A/V sync on one stream)
- **evidence needed**: does jtarget follow atarget every time?
- **status**: investigating
