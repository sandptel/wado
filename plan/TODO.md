# plan/TODO.md — the live run list

Working list for runs. The repo's `TODO.md` holds project milestones; this holds the loop.
Newest concerns first. Keep it short — close items or move them to memory.

Last updated: `2026-09-19`

---

## ▣ RUN OPEN — 2026-09-20, gamepad layout + who is connecting

Rig up, monitor armed on relay + both daemon logs. Pushed as `6af544b`, `370b6b7`, `2ada127`;
Pages deploy verified by the wasm hash moving to `dxh9a29c22acaca66f7`.

- [x] **The tunnel was dead and the rig looked fine.** No `cloudflared` since 13:21. New URL
      `specs-represented-enables-services`, baked into `DEFAULT_RELAY` (the old value was two
      rotations stale). Trap recorded in `memory/shared/environment.md`.
- [x] **The relay logs the device's real address**, not `127.0.0.1` — `peer_ip()` reads
      `CF-Connecting-IP`/`X-Forwarded-For`. Verified live: a phone joined as
      `2409:40d0:3100:afa3:8000::`, assigned daemon-2, media up in ~1 s, 1614x720@90.
- [x] **Gamepad edit mode: clusters, browser-owned layout.** Harness landed as
      `scripts/padlayout-check.mjs`. Hint text and the CHANGELOG block rewritten.
- [x] `cargo test --workspace` 133 passed · `dx build --platform web` clean.
- [x] **uinput works on this host now** — `virtual gamepad created`, six button codes clean.
      The "cannot work here yet" memory entry is withdrawn.

### Awaiting the user — only a phone can answer

- [ ] Dragging the cross moves it as one piece, and resize spreads the arms rather than
      overlapping them.
- [ ] Press feedback visible past a fingertip; a *dragged* control still animates on press.
- [ ] The layout survives a reload of the deployed client.

### Left for next time

- [ ] The monitor renders `LEAVE` as a raw log line — cosmetic, the other rules are formatted.
- [ ] `DEFAULT_RELAY` is a quick-tunnel URL and will go stale on the next `cloudflared`
      restart. A stable hostname is the only real fix.

---

## ▣ RUN CLOSED — 2026-09-19, drawer + windows fit + focus glow + pointer lock

Follow-up to the drawer run below: "applications are missing from the menu".

- [x] **The 40-tile cap was hiding 32 of 72 apps.** `MAX_TILES` 40 → 400; it is a DOM guard,
      not curation.
- [x] **`NoDisplay`/`Hidden` entries are carried, not dropped.** New `AppEntry.hidden`;
      `parse_entry` reports instead of rejecting. 71 of 143 entries here.
- [x] **👁 and ▶ on the search row.** The eye lists everything installed (hidden-marked and
      iconless), persisted as `Settings::show_hidden`; ▶ and Enter run the box, resolving a
      typed name or command against the app list first (`ui/drawer/run.rs`).

Measured: 143 apps / 869 KB per drawer open, up from 72 / 645 KB. Findings in
`memory/ui/client.md`. Daemon pool on the 2026-09-19 21:59 build. **Not committed** — the
client is not deployed with these changes yet.

### ▣ All three done, same session

- [x] **Windows overflow the output.** `crates/compositor/src/fit.rs` — `configure_bounds` on
      every new toplevel and on every reconfigure, shrink-to-fit at first commit and in
      `refit_windows`, position clamped by the *whole* window rather than its top-left.
      Measured on a 640×360 logical screen: kitty 884×1078 → 640×360, nautilus 890×550 →
      640×380. Restoring a maximized window re-fits its remembered size.
- [x] **A glow on the focused window.** `crates/compositor/src/glow.rs` — two tiled rings of
      `SolidColorRenderElement` as custom elements. Buffers live on `Wado` so a still window
      adds no damage: measured 1 damage rect/frame with the ring, 1 without.
- [x] **Pointer lock.** `zwp_relative_pointer_v1` + `zwp_pointer_constraints_v1` advertised
      (verified in a real client's registry), `InputEvent::PointerRelative`,
      `input/relative.rs`, `js/input_lock.js`, 🎯 on the bar.

### Still open, from the same thread

- [ ] **Render-time rescale for windows that cannot shrink.** nautilus stops at 380 high and
      gnome-calculator at 616; a client may refuse a configure it cannot honour, and at scale 2
      a 720p stream is below many toolkits' minimum. `RescaleRenderElement` around an oversized
      window is the answer, and it is what "force applications to respect the aspect ratio"
      actually means. **Input needs the inverse transform** or every tap lands wrong — that is
      the hard half, not the drawing.
- [ ] **kitty exits when the session is reconfigured.** Measured, and **pre-existing** — it
      happens with the fit changes stashed too. GTK apps (gnome-text-editor, nautilus) survive
      the same reconfigure. Suspect the `wl_output` global being retired and replaced; see
      `retire_output_global`. Contradicts the documented promise that a reconfigure keeps the
      session's applications.

---

## ▣ RUN CLOSED — 2026-09-19, app drawer + app isolation (lane 3, feature)

Two of lane 3's items. Both landed; the daemon pool is running the build.

- [x] **App drawer with icons.** ⊞ on the bar opens a bottom sheet: search/command box,
      recents row, icon grid. Tap launches, long-press fills the box. Icons are resolved
      server-side and carried inline as `data:` URIs — `server/src/apps/{mod,desktop,icons/}`,
      `client/src/ui/drawer/`. Details and the two traps in `memory/ui/client.md`.
- [x] **Running-app dot on the tiles.** `AppEntry.running`, filled in when the list is
      *answered* from the compositor's live child processes (`CompositorCommand::RunningApps`,
      `server/src/apps/running.rs`). Joined by exact command string, refreshed on every drawer
      open. Means "the process is alive", not "it has a window" — see the note in the code.
- [x] **X11-only apps run inside the session.** `x_server` (off by default) starts a rootful
      Xwayland as a client of the session; launched apps get its `DISPLAY`.
      `compositor/src/session_env/xwayland.rs`. Steam verified visually from an encoded frame —
      see `memory/compositor/lifecycle.md`, including the three traps that nearly read as
      "Xwayland does not work here".
- [x] **Apps no longer escape to the host desktop.** `isolate_apps` (default on): private
      `dbus-daemon` per session, `DISPLAY` removed. `compositor/src/session_env/`. Why each
      half is needed, and what the private bus costs, in `memory/compositor/lifecycle.md`.

### Not verified yet — needs a human looking at a phone

Nothing here was checked against a real device: the drawer's layout and press behaviour, and
whether a browser launched with isolation on actually opens *inside* the session. Sizing is
fluid (`clamp`) rather than stepped, with one landscape rule for short screens — that is a
claim about the CSS, not an observation of a phone.

### Deliberately skipped

- **Per-window app ids.** The dot tracks processes, not windows, so it appears a second or two
  before the window does and stays lit for an application that is alive without one. Matching
  `xdg_toplevel.app_id` against a desktop entry's `Exec` is the upgrade path, and those two
  strings disagree constantly (`org.gnome.Nautilus` vs `nautilus`).
- **A live dot.** It is a snapshot taken when the drawer opens, not a subscription.
- **Rootless Xwayland**, which needs XWM support in the compositor. Until it exists, X apps
  share one unmanaged screen. This is the next real compositor milestone if Steam is meant to
  be a first-class citizen.
- Favourites, categories, theme-aware icon lookup (the walk ignores `index.theme`).

---

## ▣ RUN CLOSED — 2026-09-19, connection hardening (lane 2)

**Headline: five of seven investigated items were not bugs, and three proposed fixes would each
have deleted something that worked.** The run's real output is a diagnosis (VPNs), a lane system,
and a set of corrected beliefs. Details below; the corrections are the part worth reading.

### Needs you (nothing else is blocked)

| | |
|---|---|
| **TURN server** | Decided and wired — `WADO_TURN_URL`/`_USER`/`_PASS` is live in `ice.rs`. **No server exists yet.** Until one does, two peers both behind a VPN still cannot connect. wado's half is reasoned, not verified. |
| **Popup grabs, steps 2–5** | Step 1 landed (additive). Steps 2–4 are the invasive swap, held for review. Step 5 (touch) has no upstream API. Plan in `memory/compositor/wayland.md`. |
| **Lane 3** | Not started. The original plan for today was lane 2 → lane 3 (wifi, audio, system settings, notifications, app launcher). Restart with `WADO_RUN=feature`. |

### Carried, unblocked, nobody waiting

- `(perf)` 111 of 120 fps reaching the client on a 0 ms path — see below.
- `(perf)` the buffer verdict flapping — see below; likely correct and merely worded badly now.
- `(compositor)` popup grabs, steps 2–5.
- Concurrent hardware-encode ceiling is still unmeasured; do that before raising
  `WADO_INSTANCES` much past 4.

### ⚠️ Corrections made this run — read these before trusting older notes

1. **"Stuck at ICE checking here means CGNAT or AP isolation"** — withdrawn. It was a **VPN at
   both ends** (WARP here, Zscaler there). Neither older theory was ever measured on this host.
2. **"The viewer watchdog has been dead for five days"** — withdrawn, mine, same day. The relay
   keepalive goes to the *client* inbox, not the daemon. Verified firing at exactly 600 s.
3. **"daemon-3 is rendering 120 fps with no viewer"** — withdrawn. `render pacing healthy` is
   tick cadence, not encoded frames; rendering was correctly paused.
4. **"The pinned UDP range is not taking effect"** — withdrawn. It is. Two of the three
   instruments used to check it could not have detected it.
5. **"The client instance marker is not deployed"** — withdrawn. Stickiness is live; observed
   `assignment="reclaimed"` repeatedly.
6. **Every smithay API fact quoted before 17:0x came from the wrong checkout.** Two exist;
   the lockfile pins `85f83ab`. Conclusions survived, by luck. See `shared/environment.md`.

---

## Run of 2026-09-19 — connection hardening (lane 2)

Run lanes exist now: `WADO_RUN=perf|connection|feature|compositor` selects a tracing filter
(`crates/server/src/runlane.rs`), passed through by `rig.sh` and logged at startup.

- [x] **The "one device never establishes media" mystery is solved on the host side: Cloudflare
      WARP gives this machine a symmetric NAT.** Three STUN servers, one socket, three different
      external ports. Every srflx candidate wado advertises is unreachable. Measured twice; a
      bypass bound to the LAN interface is blocked by WARP itself (`Operation not permitted`), so
      no code change avoids it. Detail and the withdrawn CGNAT/AP-isolation readings are in
      `memory/shared/environment.md`. `nat.rs` now says so at startup.
- [x] Per-device attribution in the daemon log: offers, answers, ICE states and the connect all
      carry `peer=<addr> room=<id>`, so a pool log can be read per device. `watch.sh` was using
      the offer candidate count as a fingerprint.
- [x] ICE `Failed`/`Disconnected` emits one line with both sides' candidate types and the elapsed
      time, instead of four lines correlated across two logs.
- [x] `scripts/watch-relay.sh` — nothing was watching the relay log, which is the only place that
      knows which device got which daemon and who was refused.
- [x] `rig.sh` reuses a live tunnel instead of rotating the URL, and warns when the deployed
      client's `DEFAULT_RELAY` is not it. That rotation stranded a phone twice, with no trace
      anywhere because the request never reaches the relay.
- [x] `rig.sh --add N` grows a running pool with no session interrupted. Verified 2 → 4 live.

### Connect timings (first concurrent numbers — for `memory/latency/measurements.md`)

| device | offer candidates | join → media | note |
|---|---|---|---|
| desktop 1728x1080@120 | 18 | **1881 ms** | cold |
| phone 1080x2422@90 | 8 | **2161 ms** | concurrent with the desktop |
| MacBook 1670x1080@120 | 15 | **never** | ICE stuck in `checking` |

### Open

- [x] **The MacBook: solved — Zscaler.** Turned off at `16:00:45`, connected in under a second.
      Offer candidates dropped 15 → 9; the extra six were the VPN tunnel. This closes the
      long-standing "one device never establishes media" item and withdraws the AP-isolation
      theory entirely. Both ends had a VPN: WARP here, Zscaler there.
- [x] **Withdrawn: "the viewer watchdog was dead for five days".** The keepalive goes to the
      *client* inbox, not the daemon, so `silent_ms` was never refreshed by it. The watchdog was
      then verified firing live at exactly 600 s (daemon-3, 16:02:14 → 16:12:17). The change made
      on the false premise was reverted; only a comment naming the real constraint remains.
- [x] **Withdrawn: "daemon-3 renders 120 fps with no viewer".** `render pacing healthy` reports
      tick cadence, not encoded frames; rendering was correctly paused. The instrument lied, not
      the code. Noted in `memory/shared/environment.md`.

- [ ] **(perf lane) A steady 111 of 120 fps reaches the client on a 0 ms path.** Observed
      2026-09-19 17:00-17:09, ten consecutive health lines: `render 120.0/120fps pump p99=0.2ms
      over=0 client fps=111.0 rtt=0ms`. A ~7.5% shortfall that does not vary, with the pump
      clean and nothing dropped, on localhost — so it is neither the link nor the encoder queue.
      The verdict calls it healthy, which is why nothing has ever surfaced it. Candidates: the
      client's own `framesPerSecond` window, or frames the compositor renders but never hands to
      the pump. Cheap to settle: compare `framesReceived` deltas against the server's sent count
      over the same 60 s.

- [ ] **(perf lane) The client verdict flaps ok↔"settling" indefinitely.** Observed 16:08–16:10
      on 2026-09-19, four flips in two minutes, ~46–51 ms behind each time, at 3.9–4.5 Mbps
      against a 3.9 Mbps target — so not starvation, and not the post-connect transient the
      wording claims. Either the threshold sits right on this link's steady state, or the buffer
      genuinely is not draining. **A verdict that calls a persistent condition transient reads as
      "nothing to do" forever** — same failure as `render pacing healthy` reporting tick cadence.
      Decide it with the jitter-buffer number, not the verdict.
      **Likely already explained:** client rtt moved 2 ms → 93 ms over the same window (16:10:18
      health line). A ~93 ms path carries ~46 ms of buffer as a matter of course, so the
      threshold is probably just below this path's steady state and the condition is permanent,
      not settling. Check rtt before assuming a fault.

- [ ] **Retry storm, now with evidence.** The failing Mac re-offers every ~12 s, and each re-offer
      **restarts the compositor session** on its daemon while holding the pool slot. This is the
      "a device that cannot connect occupies a pool slot" item below, observed for 12 minutes
      straight. A slot held by a peer that has never reached `Connected` should be reclaimable
      sooner than a streaming one.
- [x] **Withdrawn: "the pinned UDP range is not taking effect".** Measured 2026-09-19 during a
      live session by mapping `/proc/<pid>/fd` socket inodes to `/proc/net/udp`: daemon-1 held
      50010/50013/50018/50024/50025/50031/50055/50089 — all inside its 50000-50099 slice. The
      pin works.

      **It took three instruments to get one answer, and the first two lied by construction:**
      `ss -lun` output with no process attribution (the ports seen were never shown to be
      wado's); then `ss -un`, which excludes unconnected sockets and so can never show an ICE
      socket at all. A third check failed silently on a shell bug — `grep -c` prints `0` *and*
      exits 1, so `$(... | grep -c x || echo 0)` yields two lines and every `[ "$N" -gt ... ]`
      errored, reporting "no offer in 8 minutes" for an offer that had happened. **Attribute to a
      pid, and make a check prove it can detect the thing before trusting a negative.**

- [x] ~~The pinned UDP range is not taking effect.~~ `webrtc_settings.rs` sets
      `UDPNetwork::Ephemeral(50000-50400 sliced)`, but the daemon's live ICE sockets are
      `192.168.1.239:55333` and `172.16.0.2:59097`. No "pin failed" warning was logged. Not the
      cause of anything today — but a firewall rule opening the pinned range protects nothing.
- [ ] `global.stun.twilio.com` fails on every gather: `No available ipv6 IP address found`.
      It answers fine over IPv4 in 95 ms from a raw socket, so this is webrtc-ice resolving it
      v6-only, not a dead server. One of three STUN servers wasted.

## Run of 2026-09-14 — multi-device pool

**Landed.** A Remote ID is now a **pool** of daemons; each device gets its own. Detail and the
reverted-claim record are in `plan/memory/shared/pool.md`; the diagnosis rules that came out of
it are in `plan/memory/shared/environment.md`.

- [x] Monitors on daemon + relay, per instance.
- [x] **The real "devices cannot connect" bug**: the relay room was tied to a client's
      *socket*, so a tab left open with its session already stopped refused every later device.
      Measured: 14+ refusals over 4 minutes; closing the tab let the next device in **in one
      second**. Connect time was never the problem — the rejection loop was.
- [x] Pool: `registry` and `rooms` keyed by instance id, assignment prefers the device's own
      instance then the first free one, refuses with a reason when full.
- [x] Live pool growth verified: 2 → 4 daemons with two sessions streaming, no restart.
- [x] **Confirmed with real devices, 21:42** — a phone (1080x2422@90, d2) and a laptop
      (1670x1080@60, d4) streaming *simultaneously* on one Remote ID, `rooms:4 servers:4`.
- [x] Per-instance UDP slices (`WADO_UDP_SLICE`, ceiling raised to 50400). Kept because sharing
      one 101-port window across N daemons is wrong on its face — **but it was not what fixed
      ICE**, and the collision theory was measured and killed. See `pool.md`.

### Awaiting the user

- [x] **CONFIRMED 2026-09-19 — and it never needed a device.** A silent WebSocket joined through
      the public tunnel and was held 200 s with zero client traffic, receiving pings at 30.6,
      60.6, 90.7, 120.6, 150.7 and 180.7 s, with no re-join in the relay log. Before the
      keepalive the tunnel closed an idle socket every 1–2.5 min.
      **Why it sat open for five days:** the test was written as "leave a device untouched for
      ~5 minutes", and a real device is never idle — it sends stats over the same socket while a
      session runs, so the measurement could not isolate the keepalive. `node` with a bare
      `WebSocket` and no sends is the test. Script: `plan/`-adjacent scratch, 20 lines.

- [x] ~~Confirm the relay keepalive.~~ Deployed 22:14: the relay now pings idle clients every
      30 s (`KEEPALIVE` in `signaling.rs`). Before it, an idle signalling socket was closed by
      the cloudflared tunnel and every device re-joined every 1-2.5 minutes, renegotiating
      twice each time — which read as flaky Wi-Fi. **To confirm: leave one device connected and
      untouched for ~5 minutes and check the relay log has no `client joined` for it.** The
      measurement taken right after deploying was confounded by active reconnecting.

- [x] **"Is a daemon poisoned after repeated failures?" — no.** d1 carried 27 failures and then
      connected media in under a second (22:00:09) for a working device; d2 did the same at
      21:42 after 29. All four daemons have carried media. Question closed.
- [ ] **One device never establishes media — ~38 attempts, four different daemons.** It is the
      **15-candidate** device in the offers (fingerprint devices by candidate count; the
      6-candidate device on the same rig connects in ~1 s). Control plane works — the PTY shell
      opens, because that rides the relay WebSocket, not WebRTC. This host's link measured 0%
      loss to router and WAN, and every server-side theory was measured and killed (no port
      collision, no I14 exhaustion, no poisoned daemon). What is left is the path between the
      two endpoints. **Still outstanding, asked three times: `ping 192.168.1.239` from that
      Mac.** No reply ⇒ AP/client isolation ⇒ needs TURN and no wado change helps.
- [ ] **A device that cannot connect occupies a pool slot while it retries** — the failing one
      drove `pool_busy` to 4 of 4 and began refusing working devices. Worth a cheap guard: a
      slot held by a peer that has never reached `Connected` should be reclaimable sooner than
      one streaming. Pool size is also a retry-storm blast radius, not just a resource ceiling.
- [x] **Stickiness IS deployed — entry below withdrawn.** Observed live `2026-09-19 15:48`: a
      browser disconnected and rejoined as `assignment="reclaimed"` onto its own instance
      (`29eb0c39`), windows intact. The Mac did the same onto `f49ba537` repeatedly. The
      `?instance=` marker is in the shipped wasm.

- [ ] ~~**The client marker is written but not deployed.**~~ `js/relay_link.js` stores the assigned
      instance, sends `?instance=`, logs which daemon answered with the pool occupancy, and
      logs the refusal with its numbers. Until Pages ships it, **stickiness does not exist** —
      every join is `assigned`, so a device that reloads may get a different daemon and an
      empty desktop. Deploy and verify with the three-hop wasm-hash check in `environment.md`.
- [ ] **Concurrent hardware encode is the real ceiling** and is unmeasured. RAM is not the
      limit (152 MB per idle daemon on a 27 GB host). Measure before raising `WADO_INSTANCES`
      much past 4.
- [ ] **Sequential/concurrent connect timings across device types** — the run's original
      second clause. One clean number so far: **1 s** join→ICE-connected on a local browser,
      cold. Phone and laptop concurrently, and the refusal latency when full, still to capture
      into `plan/memory/latency/measurements.md`.

## Older — carried forward

## Awaiting the user (blocked on a human, not on work)

- [ ] **PTY shell end to end.** Open the console → Shell tab. Expect a prompt, working
      `vim`/`top`, Ctrl-C interrupting, resize reflowing. Server and client are both
      deployed and byte-verified; nobody has watched it run.
- [ ] **Pixelation on scroll at 1080p** — still present after the release-build fix and the
      resolution-aware bitrate? This decides whether the bitrate work is finished.
- [ ] **v0.0.1 tag predates the flake**, so the tag cannot be `nix build`-ed. Move the tag,
      or let packaging land in v0.0.2? Moving a published tag is normally bad practice.
- [ ] **Track the planning docs?** `CLAUDE.md`, `TODO.md`, `WADO_PLAN.md`, `CHALLENGES.md`
      are gitignored, so edits to them never reach a clone. Deliberate, or an accident?
- [ ] Two-finger scroll — implemented, never confirmed by a human.
- [ ] **Fractional scale actually applies now** (8f363e6, integer half floored in afc7c80).
      Connect a phone at scale 1.25 and check the daemon log says
      `compositor session active … scale=1.25` with no `applied=` line, app chrome is not
      clipped, and touch still lands where it is put.
- [ ] **Everything is running at scale 1.75 through Chrome, which speaks fractional scale, so
      the integer fallback has never been exercised.** `new_fractional_scale` now logs each
      surface that takes the fractional path. If an app appears in a session and never shows up
      in that log, it is the population `floor` was chosen for — worth a look at its chrome.
- [ ] **Pinch-zoom reaches apps now** (198faca, 1dd7c6f). Pinch in a map or image viewer and
      check it zooms — and that the same two-finger gesture still pans, without panning twice.

## Evening continuation of 2026-09-12 — from 18:00

Five fixes, all deployed (daemon swapped 20:45, Pages green at 15:13 UTC). Details in
`issues.md`; the architectural one is in the Decision Log.

- **Session lifetime is no longer tied to transport lifetime.** A `Failed`/`Closed` peer
  connection stopped the compositor *immediately* — every cell handoff cost the viewer its
  windows, its applications and a cold Chrome launch (17 launches in one daemon run).
  `viewer_watchdog`'s 45 s grace is now the only teardown.
- **Relay-mode WebRTC recovery never worked.** `handleFailure` and `resync` both called
  `connectWebRTC()`, which POSTs to `/offer` — direct-mode only. Every relay reconnect spent
  three retries on an impossible fetch. `W.reconnectWebRTC()` chooses the path;
  `scripts/reconnect-check.mjs` pins it. Retry budget 3.5 s → ~37 s to fit inside the grace.
- **ICE port exhaustion (I14).** Dropping an `RTCPeerConnection` frees nothing in webrtc-rs, and
  ICE is pinned to 101 ports. 4 leaked per negotiation; at ~25 the daemon answered `0 candidates`
  and no client could connect. That is what the user hit at 18:19. Confirmed fixed by
  measurement: 2 negotiations, still 4 ports.
- **Keyboard auto-detect (I4).** `--enable-wayland-ime` added at the launch choke point. The
  earlier withdrawal of this claim was wrong — it refuted *binding*, and what is missing is the
  `enable`.
- **Client-driven fps backoff (I1).** Shipped — see below.

**Still needing a human, both deployed and both unseen working:**
- `⌨ TEXTIN` appears every session; `text input focus changed` never has. Until one line shows
  up, the IME flag is a hypothesis.
- `◇ STRAIN` then `⚠ SHED … the PHONE asked for it` three windows later. If STRAIN appears and
  SHED never does, the client is toggling the flag rather than holding it, which resets the
  window counter — the one failure a unit test fed a constant cannot catch.

**Next run starts here:** I8 (the jbuf ratchet) is now genuinely one tap, because ⟳ Resync was
repaired as a side effect of the reconnect work — it had never worked in relay mode. The verdict
strip could offer the action rather than the setting. I10 (`surface missing from known popups`)
is the only unexplained recurring error left now that the connection path is solid.

## From the roaming run of 2026-09-12 — closed 17:25

**Everything reported-and-unfixed now lives in `issues.md` (tracked, repo root), I1–I9.**
Measurements in `reports/2026-09-12-latency-roaming.md`; durable findings promoted to
`memory/latency/measurements.md` and `memory/shared/{metrics,verification}.md`.

Shipped and deployed this run: monitor rewrite with side attribution (`scripts/watch.sh`), the
continuous sampler (`scripts/sample.sh`), the health verdict with bandwidth + suggestions,
`zwp_text_input_v3`, the ⌨ label fix, and sleep inhibition.

**Still needing a human:** the auto-keyboard has never been seen to raise a keyboard — Chrome
binds the protocol (confirmed) but no `text input focus changed` line has ever appeared, so it
is unknown whether Chrome *uses* it. `issues.md` I3 covers the untested compositor path.

~~**Next run starts here:** I1 (fps backoff)~~ — **done**, 20:44. The trigger landed as the
settled verdict rather than a raw duty threshold, which brought `SETTLE_TICKS = 3` along for
free, and `STRAIN_WINDOWS = 3` was added on top because a latched 1 Hz level acted on
per-window walks to the floor in under a second at 90 fps.

## Superseded — from earlier in the run of 2026-09-12 (see `reports/2026-09-12-roaming-run.md`)

- [ ] **Nothing in wado reacts to a receiver-side collapse.** A2: the server pushed 90 fps into
      a decoder managing 15, for 86 seconds, and never noticed — server metrics were perfect
      throughout. The client measures `dec` and `framesDropped` and only *displays* them.
      **Client-driven fps backoff** is the fix and it is small against this architecture: the
      verdict already exists (`js/health.js`), the data channel already carries client→server
      messages, so it is one protocol message, one match arm in `relay_client`, and the
      compositor already rebuilds an encoder on a settings change. Decide the hysteresis before
      writing it — a backoff that oscillates is worse than none.
- [ ] **Is the A2 collapse thermal?** Reconnect on a cool phone at the same settings; then at
      60 fps. Recovery at 60 and collapse-after-~80s at 90 both point at duration-dependent
      throttling. Immediate collapse on a cool phone kills the theory.
- [ ] **Auto-raise the keyboard when an app takes text focus.** Needs `zwp_text_input_v3` *and*
      an `input_method_v2` instance — smithay drops every text-input request when none is bound
      (`text_input_handle.rs:209`, `has_instance()`), and both modules exist at the pinned rev.
      The manual ⌨ path landed first (`fc386bc`); this is the automatic half the user asked for.
- [ ] **`backend=` on the session-active line is the *requested* backend, not the selected
      tier.** It logged `Auto` on a session that was really VA-API, which reads as an encoder
      that never resolved. One-word fix; log the tier.

## Next up

- [x] **Chrome takes the dmabuf path** — `AB24` under an AMD tiled modifier, confirmed live on
      the first session of `2f55d82`. R10 is closed.
- [ ] **How much is it worth?** Still open: the saving lands in the client and the texture
      upload, not in the stages `timing.rs` breaks out, so look at host CPU (`/proc/<pid>/stat`
      utime delta) across the `checkpoint-pre-dmabuf` tag and today, with the
      `compositor session active` lines identical. → `optimisation.md` O7.
- [x] **Bits per pixel is on the session line** (`9fb5b99`). Unit-tested; the live line has not
      been read yet — no session has run since the daemon carrying it started.
- [x] **Panel refresh rate measured and shown** (`8da467b`), with a warning when the chosen fps
      exceeds it. Byte-verified in the deployed wasm; **awaiting a human**. Unblocks R8.
- [x] **Resync button shipped** (`8da467b`). Byte-verified in the deployed wasm; **awaiting a
      human** — pressing it after a hitch and watching jbuf drop is the O9 measurement.
- [x] **120 fps at matched settings — done 2026-09-12.** `dec` ~7.0 ms, which is *not* the
      ~5 ms proportionality predicted: decode scales with bits per frame down to ~63 kbit/frame
      and then flattens at a ~7 ms floor. 120 fps therefore spends 84% of its decode budget
      against 60–62% at the lower rungs. It still fits; it is no longer free.
      → `optimisation.md` O8.

- [x] **Phone soft keyboard, the cheap half** (`8da467b`). ⌨ in the bar focuses a hidden input.
      Two traps found: the input cannot be truly invisible or no keyboard rises, and **Android
      reports an empty `KeyboardEvent.code`** so characters are mapped from the `input` event.
      US layout only. Byte-verified in the deployed wasm; **awaiting a human**.

- [ ] **The shell dies on every reconnect — is that what "multiplexer like" meant?** The
      `Pty` is a local in `connect_and_serve`, so losing the relay connection (or restarting
      a session, which the user does constantly while testing) drops it and the next Shell
      tab gets a fresh bash in `$HOME`. That is correct for *cleanup* and wrong for a
      multiplexer: surviving a disconnect and reattaching is tmux's defining feature, and
      the request was "a terminal multiplexer like setup".
      Making it persist means moving the `Pty` up to `RelayCtx` (lives across reconnects),
      keeping the scrollback server-side, and replaying it on reattach — and deciding when a
      detached shell is ever killed, or it becomes the process leak that was just fixed.
      **Answered 2026-09-12: persistence AND panes.** That makes it a milestone, not a run item
      — scoped in `reports/2026-09-12-deferred.md`, including the kill rule it needs and its
      interaction with `viewer_watchdog`.

- [x] **Dead exec path deleted** (`459dfca`) — 272 lines across four crates, after confirming
      nothing called `W.relayExec` and no UI read `term`/`term_input`/`term_busy`. Supersedes
      `e05aac6`, which had tied an exec child to its connection: there is no exec child now.
- [ ] **Confirm the build-starvation theory.** Stalls of 140–242 ms on 6–12 KB frames
      appeared exactly while `cargo`/`nix` builds saturated all 20 cores, and the client
      stayed healthy throughout. Run a build deliberately during a session and watch.
      If confirmed: never build while the user is testing, and say so in memory.
- [x] **Stall instrumentation uncensored** (`62aeaa9`). Every frame recorded; p50/p90/p99/max per
      300 frames. Unit-tested, including the case the threshold log could never show. A real
      stall cannot be forced, so the live path ships unverified.

## Newly surfaced by tracing (2026-09-11, from `memory/latency/measurements.md`)

- [ ] **720×1614 @120 dropped 2497 frames; 1728×1080 @120 dropped none.** Server clean in
      both (0 stalls, qmax 1) and the *larger* output is the healthy one, so it is not
      bandwidth or encoder load. **The decoder theory is dead** — `dec` scales with bits per
      frame, not resolution, so 120 fps decodes *cheaper* per frame than 60. Needs a new cause;
      the VBV A/B is the next test. → `research.md` R1, `optimisation.md` O8.
- [ ] **Software tier cannot hold 1080p120** (tick 8.3→22.5 ms vs an 8.33 ms budget) yet the
      UI offers the combination. Constrain, warn, or leave as an experiment surface?
      → `research.md` R7.

## Wayland protocols — what is left

Ranked in `memory/compositor/wayland-protocols.md`. One protocol per change, one measurement
each; do not batch them.

- [x] **`zxdg_decoration_v1` — server-side, borderless** (`f613313`). User chose it. A client
      asking for client-side is told server-side anyway; the compositor's configure is
      authoritative and an app insisting on its own titlebar would reintroduce the strip this
      removes. **Awaiting a human:** Chrome should have no titlebar, no shadow, no frame.
- [x] **`xdg_activation_v1`** (`4667693`). The launcher's half of "raise the window I just
      started". Every token honoured (all clients here are session-spawned), surface looked up in
      `space` first, token removed after one use.
- [x] **`wp_presentation`** (`7f551e2`). The risk this was parked on was the clock, not the
      protocol: `start_time.elapsed()` sits four lines from the hook point and would have put
      every frame one daemon-uptime in the past. Timestamps read `CLOCK_MONOTONIC`, flags are
      empty (no scanout to claim), `Refresh` comes from the output mode.
      **CONFIRMED from the log, `2026-09-12` 17:47** — no human needed after all:
      `presentation feedback answered — a client is pacing on our timestamps surfaces=1 seq=186`,
      and the frame rate settled to exactly `60.0/60, mean 16.7 ms` (58.0 → 56.2 → 58.8 → 60.0)
      with client `dec` at 10.76 ms of a 16.6 ms budget. Chrome is pacing on wado timestamps and
      neither the render loop nor the decoder is disturbed by it.
- [x] **`wp_single_pixel_buffer_v1`** (`7b5c5e3`). One global; the renderer already handled
      `BufferType::SinglePixel`.
- [x] **`wp_content_type_v1`** (`002fc97`). Advertised and **logged on change only**; nothing
      reads the hint. The log is what answers whether any app sets one — until then "should the
      encoder act on it?" has no evidence either way.
- [ ] `wp_cursor_shape_v1` — **skip permanently.** wado draws no cursor (`cursor_image` is an
      empty body), so both halves of the protocol are no-ops here.
- [ ] `wp_fifo_v1` / `wp_commit_timing_v1` — **deferred on a real blocker**, not on value. Both
      require the render tick to withhold a ready surface from the composite it is building — a
      per-surface barrier inside `render_output`. The tick has no concept of "ready but not due".
      Advertising them without the barrier gives an app *wrong* pacing instead of none.

## 🛑 Run stopped 2026-09-12 after v0.0.3

Handoff: **`/tmp/wado-handoff-2026-09-12.md`** — read it with this file.

**Shipped in v0.0.3** (`dd4d4c1`, tagged): four Wayland protocols, the scroll units fix, rejoin-or-
drop on reconnect, render-tick shedding under congestion, a dimension clamp, and `scripts/rig.sh`.
Prose in `CHANGELOG.md`; do not restate it here.

**Rig left running** on the user's instruction — daemon on the 13:34 v0.0.3 build, relay 9296,
tunnel 9320, `https://fought-instead-carolina-calculate.trycloudflare.com`, Remote ID 872-990-894.
Use `scripts/rig.sh --daemon` to restart the daemon without rotating the tunnel URL.

**⚠️ Before running any queued measurement:** this link went from zero drops in 4 m 45 s to sixty
in 47 s on an identical config. Minutes-long A/B here is confounded, and a conclusion drawn from
two five-minute windows had to be withdrawn during this run. Interleave or repeat; the tooling
does neither yet.

## ⛔ Awaiting a decision from the user — none of these are mine to pick

### 1. Congestion control (R6 is **answered**: the ceiling collapses on cellular)

`CEILING_KBPS = 12_000` is not protection, it is a constant guarding against something variable.
Measured 2026-09-12: 420+ frames dropped, keyframes stalling 829 ms, encoder asking 1.5 MB/s of a
link delivering 60–270 kB/s. Full evidence and the three options in `research.md` R6.

- [ ] **Decide:** lower the ceiling / real RTCP-driven BWE / ⭐ a local controller off the
      `write_sample` stall and pump-drop signals we already compute. Recommended: the third, as a
      first move, with its oscillation risk stated rather than hidden.

### 2. XDG desktop portal inside a session

`snapshot`'s camera fails every launch: `org.freedesktop.portal.Desktop` is not reachable on the
session bus. Needs the portal **and** PipeWire. Possibly tractable — the Camera portal is
implemented by `xdg-desktop-portal` itself rather than a desktop-specific backend — but wado has no
session D-Bus setup of its own, so it needs trying, not promising.

- [ ] **Decide:** is portal support in scope at all?

### 3. Popup grabs — see below. Real, user-visible, and unstarted.

### 4. TURN / relayed media

`environment.md` records the mechanism: two CGNAT endpoints cannot hole-punch and wado offers only
`(host, srflx)` candidates. Needs a TURN server or media forwarding in `wado-relay` — the latter is
already the stated architecture and is not built.

- [ ] **Decide:** which.

## Popup grabs are not implemented — found live 2026-09-12

`XdgShellHandler::grab` in `handlers/xdg_shell.rs:123` is an **empty body**. wado accepts the
request and does nothing, so no popup ever gets a grab and `send_popup_done` is never sent.

**User-visible consequence:** tapping outside an open menu does not dismiss it. On a phone that is
a menu you cannot close without finding the app's own dismiss affordance. Chrome hides this
because it dismisses its own menus; a GTK app would not.

Surfaced by `ERROR smithay::wayland::shell::xdg: surface missing from known popups`, seen three
times in one Chrome session. The message itself is **not** the bug and is not worth silencing:
smithay looks the popup up in `known_popups` to hand it to `XdgShellHandler::grab`
(`shell/xdg/mod.rs:2039`), the entry is already gone, and it logs instead. Since our `grab` does
nothing, the missing lookup changes no behaviour — it is a symptom pointing at the empty handler,
not a fault in it.

- [ ] Implement the popup grab. **Scoped 2026-09-19 — this is not a handler fill-in, and the
      entry above understated it.** Two findings:

      1. `PopupManager::grab_popup` requires `SeatHandler::KeyboardFocus: From<PopupKind>`, and
         wado has `type KeyboardFocus = WlSurface` (`handlers/mod.rs:28`). `From<PopupKind> for
         WlSurface` cannot be written — both types are foreign, so the orphan rule blocks it.
         Using the upstream grab means introducing wado-owned focus enums
         (`KeyboardFocusTarget`/`PointerFocusTarget`, as anvil has) and threading them through
         pointer, touch, keyboard, state and every existing grab. That is an architectural
         change and needs agreement, not a patch.
      2. The pinned smithay has `PopupGrab`, `PopupKeyboardGrab`, `PopupPointerGrab` — and **no
         `PopupTouchGrab`**. The touch angle noted above is not "wire it deliberately"; there is
         no upstream API for it at this revision.

      **Cheap alternative, ~15 lines, fixes the symptom only:** `PopupManager::dismiss_popup`
      called from `input/touch.rs` on `TouchPhase::Down` (and the pointer equivalent) when the
      surface under the point is not itself a popup. **Not proposed without a decision**: it
      would dismiss *every* open popup on any outside input, where a correct compositor dismisses
      only popups that asked for a grab. Chrome manages its own menus today and works; this could
      regress that. ⇒ **Awaiting the user:** symptom fix now, or the focus-enum refactor properly?

## Optimisation candidates (measure first, none is justified yet)

- [ ] Keyframe cadence — a decision was documented in `x264enc.rs` and never made.
- [ ] `worst_queue_ms` spikes to 200+ under load: is the 2-slot frame channel right?
- [ ] Input latency has never actually been read — needs Debug → latency on, then
      `input(rt)=` arrives over the relay.

## Known ceilings (accepted, not bugs)

- An app that calls `setsid` escapes the process-group cleanup (seen: `chrome_crashpad`).
- PTY output is UTF-8 text; non-UTF-8 bytes become replacement characters. base64 is the
  upgrade path.
- No exit code on `PtyExit` — the reader thread notices the exit and does not hold the child.
- Relay is not publicly deployable: ~30-bit Remote ID is the only secret.

## Resource governance (2026-09-11, landed but undeployed)

- [ ] **Read `runq_ms` — deployed, n=1 so far.** One stall: `took_ms=162 runq_ms=0 psi_cpu=0.0
      psi_mem=0.0`. Consistent with "not CPU", nowhere near enough to close the branch point.
      The new percentile line should generate the volume.
      - `runq_ms ≈ took_ms` → CPU starvation. `WADO_APP_CPU_WEIGHT` is a real fix; tune it,
        then consider exposing it as a session setting.
      - `runq_ms ≈ 0` → not CPU. Say so, do not tune weights, and follow `psi_mem`
        (reclaim from a browser allocating) as the next suspect.
- [ ] **`WADO_APP_CPU_WEIGHT` default is 50, unproven.** No number justifies 50 over any
      other value yet. Do not present it as a fix until `runq_ms` has spoken.
- [ ] Tokio worker cap remains **rejected** (research R3) — still no measured problem, and
      `runq_ms` is what would produce one.

---

## `graceful` branch — merged to main 2026-09-13

Three asks, all shipped and deployed. Full write-up in [`gracefulness.md`](gracefulness.md).

- [x] **Session survives a disconnect.** The killer was `relay/src/signaling.rs` synthesizing
      `session_stop` on any socket close. Field-confirmed by the user: reconnect → streaming in
      8 s with the window intact, nothing pressed.
- [x] **The relay link is persistent.** `js/relay_link.js` — dials at page load, reconnects
      forever with backoff. A `wado.watching` crumb carries a session across a page reload.
- [x] **Bitrate and aspect ratio change on the fly.** `Reconfigure`, 6–50 ms, applications
      alive. **Apply** button next to Start/Stop.
- [x] Hardening pass: 9 error paths closed, each with a check that fails without it.

### Open, from this work

- [ ] **I16** — `kitty` exits when the output is replaced. Chrome does not. Next step is
      reading kitty's source, not wado's.
- [ ] **I17** — two viewers both auto-rejoining now fight over the peer connection.
- [ ] **A failed `reconfigure_session` is untested against a real failure.** 8K was *accepted*
      by this hardware, so nothing could be found that fails the pipeline build deterministically.
      The `render_tick` guard is reasoned, not measured.
- [ ] ⛔ **A daemon restart still kills the desktop** (SIGTERM → `stop_session`). `rig.sh
      --daemon` is the normal deploy step, so every swap ends the user's session. Lifting it
      needs the supervised child process milestone.
- [x] **`SentKbps` client half** — done and deployed. The verdict now splits three ways; the
      two bugs the new cases caught (`+null === 0`, and a harness that set the value before the
      reset wiped it) are written up in `gracefulness.md`.
- [ ] **Deploy I19** (`Congestion::reattach`) — committed at `633502b`, held because the swap
      kills the user's live session. One `scripts/rig.sh --daemon` when they are idle.
- [ ] Long-session field soak: no spurious timeout has been *observed*, but nor has a long
      session been watched since the link work landed.

---

## ▢ Frame-rate ↔ refresh-rate sync — asked `2026-09-13`

*"the fps if matched to the compositor refresh rate does that make the drop normal (while
scrolling)? if we implement the functionality via which fps and refresh rate sync up … would
that work or make things worse?"*

### What is already true, and must not be re-derived

**There is no separate compositor refresh rate to sync to.** wado is headless; the output's
advertised mode is `refresh: ec.fps * 1000` (`headless.rs`), the render timer is
`1_000_000_000 / ec.fps`, and the encoder is `ec.fps`. All three are one number. Server-side the
sync the question asks for is already total.

**The client already measures the panel's real rate.** `js/refresh.js` takes the median
`requestAnimationFrame` gap and snaps to a known rung. It is **advisory only** — emitted to the
Dioxus fps picker, never sent to the server, and nothing constrains anything by it.

### The two places the rates genuinely do not line up

**1. Shedding desynchronises the output from reality — the strongest candidate for "drops while
scrolling".**

The congestion divisor changes how often the render tick *renders*. It does **not** change the
advertised mode. So at divisor 4 on a 90 fps session the output still says 90 Hz, the apps still
get frame callbacks at 90 Hz, Chrome still renders a scroll animation at 90 fps — and 67 of every
90 frames are thrown away after the client drew them.

Two costs, and the second is the one that would be *felt*:

- wasted work on the app side, on the machine already under load;
- **the app's animation is timed against 90 Hz while the viewer sees 22.5**, so a scroll is
  sampled at a cadence it was not authored for. That is not "fewer frames", it is *uneven*
  frames, which is exactly what "drops while scrolling" describes.

Note the shed moves every few seconds under a bad link, so a naive "change the mode on every
divisor change" would be a mode-change storm. `congestion.rs` already tracks stability
(`patience`, `RECOVER_WINDOWS`) and that is the hysteresis to hang this on.

**2. Beat frequencies against the phone's panel.**

| session fps | 60 Hz panel | 90 Hz | 120 Hz |
|---|---|---|---|
| 90 | 1.5 — **uneven**, and a third of the frames cannot be shown at all | 1:1 clean | 1.33 — **uneven** |
| 60 | 1:1 clean | 1.5 uneven | 2:1 clean |
| 45 (shed 2 of 90) | uneven | 2:1 clean | 2.67 uneven |

Sending 90 fps to a 60 Hz phone is not a better experience bought with bandwidth — **a third of
those frames are undisplayable**, so it is a third of the bitrate, the encode and the decode
spent on nothing, plus an uneven cadence. On a 120 Hz panel 90 fps judders even though every
number in the log looks healthy.

The pleasant property: if the **base** fps divides the panel evenly, the shed ladder inherits it,
because the divisors are 1/2/4.

### Verdict on the question

**It would work, and it is worth doing — but in three steps, smallest first, and measurement
before policy.** The reason to be careful is that two of the three levers are already adaptive
(`congestion`) and adding a second adaptive loop on top of an existing one is how the strain
oscillation of `2026-09-12 21:43` happened.

- [x] **Step 1 — measure, change nothing.** *Done `2026-09-13`, and it needed no protocol
      message at all.* The client already knows both numbers, and `rlog` already reaches the
      daemon log, so `hz=` and `ratio=` went onto the stats line next to `vis=` — **client-only,
      no daemon restart, nothing to interrupt.** The original plan said "send `refreshHz` to the
      server", which would have cost a `RelayMsg` variant, a relay pass-through, a server arm and
      a swap that kills the live session. Worth remembering: *a measurement does not need a
      protocol just because the answer is wanted server-side.*
- [x] **Step 1b — the frame-rate lock.** *Shipped `2026-09-13`, `33e19bb`, client-only.* Not in
      the original plan; asked for directly after a 120 fps session on LTE walked its rate. A
      checkbox under FPS that makes `health.js` never report strain, so the divisor never leaves
      1. It does not *sync* the rates — it stops one of them moving, which is the part that can
      be had today without a daemon swap. Full reasoning in `plan/sync.md` §1; the problem it
      answers is `plan/problems.md` §P1, which found a **third** unsynchronised rate the analysis
      below missed: the input coalescer paces sends at the *phone's panel rate* and its own
      comment asserts that this equals the remote render rate, which shedding made false.
- [x] **Step 2 — pick a base fps that divides the panel.** *Shipped `2026-09-13`, `b3d9723`,
      client-only.* Each rung now carries its cost on the measured panel — "120 — 50% never shown
      on this 60 Hz screen", "60 — even on this 60 Hz screen", and "uneven" for the 3:2 case
      (60 on 90 Hz), which is invisible in every metric and stutters. Four unit tests in
      `ui/session.rs`.

      **Stopped short of auto-defaulting**, deliberately: a picker that silently overrides an
      explicit choice is the same class of surprise the frame-rate lock exists to remove. The
      labels make the right choice obvious without making it for anyone.

      **Why this became urgent rather than a nicety:** Step 1's telemetry came back `hz=60
      ratio=2.00` — the session had been running 120 fps into a 60 Hz panel, discarding half of
      every rendered, encoded and transmitted frame unseen, for the whole investigation. The
      warning saying so was already on screen, below the control, and went unread. See
      `plan/problems.md` §P1b.
- [ ] **Step 3 — let the advertised mode follow a *settled* shed.** Only after a divisor has
      held for several recovery windows, and refresh-only (never a resize — invariant #8 and the
      kitty finding, I16). `reconfigure_session` is the mechanism and already costs 6–50 ms with
      the applications alive. **Ship behind a flag**: this is the one that can make things worse,
      because apps re-pace on a mode change and doing that under a flapping link is the failure
      `congestion.rs` exists to prevent.

      **Blocked on I16**, as of `2026-09-13`: a refresh-only change still goes through a fresh
      `Output` (invariant #8), and a fresh `Output` makes kitty exit. Smoother pacing is not
      worth a dead terminal, so I16 is now the gate on this entry rather than a neighbour of it.

⚠️ **Do not implement step 3 before step 1 has produced a number.** — *Step 1 has now produced
it: `hz=60 ratio=2.00`.* Not 1:1, so beat frequency was real and was in fact the dominant waste.
Step 3 remains blocked on I16 for its own reasons (a fresh `Output` kills kitty), but the
premise it was waiting on is settled.


## ▢ A keyframe per visibility flap (noticed `2026-09-13 13:05`, not urgent)

`set_viewer_visible(true)` forces an IDR on every resume. Observed in the field, a phone
switching apps flaps the flag faster than that is worth:

```
07:35:06  back on screen — rendering resumed
07:35:09  went off screen
07:35:12  back on screen — rendering resumed
07:35:13  went off screen
```

Four transitions in seven seconds, two forced keyframes. A keyframe here measures ~40 kB against
~7 kB for a P-frame, so a flap costs roughly six ordinary frames of bitrate — spent at exactly
the moment the viewer is switching apps on a mobile link.

**The IDR is probably unnecessary for a short pause.** Nothing was *sent* while paused, so the
decoder's reference frame is still intact; it has not lost state the way a genuinely new decoder
has. The keyframe is belt-and-braces inherited from `set_viewer_attached`, where it *is* needed
because the peer connection is new.

- [ ] Force the keyframe on resume only when the pause was long enough to matter (a second or so),
      or when the frame is stale for some other reason. Cheap, and it wants measuring rather than
      guessing: log the pause duration next to the resume line first.

**Measured `2026-09-13 14:05`, and it shrinks this entry considerably.**

The log shows **2-4 `forced IDR keyframe requested` lines per transition** — four at 08:32:20,
08:32:28 and 08:32:39 — which looks like a burst of four keyframes at the worst possible moment.
It is not. Five call sites can fire on one attach (pc `Connected` in `relay_client.rs:1017`,
`set_viewer_attached` and `set_viewer_visible` in `headless.rs:855`/`:879`, plus the browser's own
RTCP PLI at `:992`), but they all land on **`force_idr = true`, an idempotent boolean**. Four
requests set the same flag and the next submitted frame is one IDR.

The pump reports confirm it independently: `frames=300 keyframes=2` / `keyframes=3` against a
120-frame keyframe interval is 2.5 expected — **the periodic rate, with no extra bursts anywhere**.

So the cost of a flap is *at most one* keyframe, not four, and the message logs a **request**
rather than an emission. Two corrections to this entry's arithmetic:

- measured keyframe size is **22-23 kB** in the common case (`key_avg_kb=22`), reaching 48 kB on
  busy content — the "~40 kB" figure above came from a different session and is not typical;
- a flap therefore costs ~3 ordinary frames of bitrate, not six.

Still worth doing — one needless 22 kB IDR on a 2.6 Mbps link is ~70 ms of transmit — but it is a
small optimisation, not the reconnect-cost problem it looked like. **Do not swap a daemon for it
alone.** The one genuinely cheap improvement is making the log line say `requested` vs `emitted`,
because four DEBUG lines that mean one keyframe will mislead the next reader exactly as they
misled this one.

⚠️ Do not "fix" this by debouncing the *hide* instead. Pausing late means continuing to encode
and transmit for a page already off screen, which is the waste I20 exists to remove — and the
hide is the valuable half.
