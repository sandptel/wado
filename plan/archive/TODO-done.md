# TODO-done.md — closed items moved out of `plan/TODO.md`

History only. Not read during a run; grep it when a past decision or commit hash is needed.
Moved `2026-11-12`, grouped under the heading each item sat under.

## ▣ RUN — 2026-11-09, macOS host without the compositor (branch `macos`)

- [x] Daemon builds/runs on macOS; Linux CI (nix) + macOS CI green per push.
- [x] Platform + capability states on the wire; Mac providers: input (trackpad interpreter, keys,
      text, gamepad-as-keys), clipboard, files (trash, thumbnails), media, volume, brightness,
      sysinfo, awake/lock/sleep, launcher, screenshot, shells.
- [x] Wado.app signed `wado-dev`; `wado install` exists, **not enabled** (user: later use).
- [x] App macOS variant ("Coming to macOS" / "On this Mac"); Apple logo.
- [x] Web variant (`crates/client/src/host_caps.rs`, "On this Mac" card). APK `~/Downloads/wado-0.4.0-release.apk`
      (signed with this Mac's new debug key — uninstall the old app first; pairings re-done).
- [x] Grants given (Accessibility, Screen Recording). `node scripts/mac-e2e.mjs` green: pairing, platform,
      caps, outputs, clipboard both ways, touchpad moved the real pointer (+128 pt).
- [x] Real Dock gestures (Spaces / Mission Control / Show Desktop / Launchpad) per the Mac's Trackpad settings; measured signs (`plan/memory/shared/macos.md`).
- [x] Direct connections: IPv6 found on macOS (getifaddrs). TURN optional: `turn { url … }` or `cloudflare-key`/`cloudflare-token`; devices fetch servers with `ice_get` (app + web).
- [x] Jitter: relay pacer delay up to 120 ms; smoothed pointer speed; staggered two-finger tap right-clicks.
- [x] Touchpad haptics per gesture (app: Android composition primitives; web: vibrate patterns).
- [x] Media: mediaremote-adapter (any app, art, live), pushed to the viewer as it changes (`pushed` flag).
- [x] Ponytail cleanup (−168 lines; CG via objc2).
- [x] File manager, both clients: sort name/date/size/type + folders first, list/grid (3 sizes) with thumbnails, hidden, breadcrumbs, filter, cut/copy/paste bar, duplicate, undo (move/copy/rename), select all/invert, properties, pins, folder .zip, trash + restore. Web adds drag-to-move, arrow keys, Ctrl+X/C/V/Z/D. Server: move/copy answer `{to}`, clashes refused up front. Checks: `scripts/files-ops-check.mjs`, `files-order-check.mjs`, app `test/files_view_test.dart`.

## ▣ RUN — 2026-11-05, Android app smoothness (ui lane; code review, no phone attached)

- [x] `Input._arm` now a microtask (was frame callback + forced frame). Watch input(rt) for change.
- [x] Rust: room messages embedded as JSON (one Dart parse); repeated `host_state` → `{same:true}`
      on the event thread (`crates/app-bridge/src/repeats.rs`). `send_room` already only enqueues.

## ▣ RUN — 2026-11-04, player home / Game mode / delegated pairing (autonomous; Decision Log 2026-11-04 ×2)

- [x] Web: pad per game (`P.shadow`), poster-grows launch, Devices QR (`wado_ux::qr::path`),
      control-room pairing (verified live 2026-11-05), tablet/desktop pass by CDP screenshots.

## ▣ RUN — 2026-10-30, after-trackpad features (orchestrated; ping the user after each)

- [x] F3 — Passkey for shells + Claude status cards + push ✓ 2026-10-30 (merged; phone checks open)
- [x] F4 merged (reconnect 4254→862 ms median; lane; relay health; compact frames; idle seat). Needs relay redeploy (user). Not verified on a real phone handoff.
- [x] F5 — Pocket Console merged. Open: phone fps/decode-drop console vs cockpit; `maximized` in WindowInfo; people card; computed contrast + SR pass
- [x] F11 merged (gamepad-check 27/27, layers 513/0, console 91, responsive 0 failures). LEFT for handover:
  - T19 real-hardware check: phone, a game, evtest, a live `hyprctl keyword`, the stuck-button-after-kill case, latency.
  - gamepad-check has no daemon phase.
  - Sessionless Keys & mouse (T18): `KeyInjector::press` and a relative pointer.
  - Behave tab key rebinding; the widget tinting the last door; the native trackpad's "More" opening the Touchpad page.
  - Re-apply the Hyprland feel on `configreloaded`; Lua config (`hyprctl eval`).
  - Click bar and thumb marking: settings stored, caps `pad-click`/`pad-palm` not built.
  - Research follow-ups: frame timing per batch, Wi-Fi low-latency lock, gesture mirroring, threshold haptics.
- [x] Session trap + navigation + recording defects merged (trap-check 19/19). LEFT for handover:
  - Wire `pending.js`/`pending.rs` into every action (chokepoints are listed in the agent's report and HANDOVER.md), plus `feedback-check.mjs`.
  - Don't tear the session down on an unrelated `session_error`.
  - Console perf (baseline: press 111–304 ms, input-to-paint up to 256 ms at 4×, 608 ms at 6×).
  - layers-check: add the new states.
  - responsive-check at 390×844: 36/240 failures (gamepad `.gpc`, `.vidoorname`), from F11.
- [x] Passkey never-stuck merged (eaf023f + d18f3f3): a single bounded prompt, plain errors, approve on the computer (5 per 10 min), per-device passkey off; the two-tap race fixed (any live challenge counts). passkey-ui-e2e 95/95, f3-e2e all pass. Daemon swapped (pid 2191734).
- [x] Passkey fixed (sheet was inert: Dioxus inert="false"), lifetime choice, trackpad refusal cards — merged
- [x] Layering fixed (layers-check 98→0) + file viewer revamp (11 kinds, zip listing) — merged. Unverified: real-phone pinch, Safari HEIC, Firefox PDF frame. files-e2e passkey path flaky under load.
- [x] Keep-awake leaked systemd-inhibit holds across daemon restarts (systemd-inhibit blocks SIGTERM → PDEATHSIG swallowed); fixed with a stdin-pipe EOF (791c09a). Two old holds (637729, 1031976) left for the user to kill.
- [x] Size: text + interface size per device (cozy default), reflow fixed (responsive-check 251→0 failing states), Android textZoom pinned — merged. Unverified: iOS Safari (zoom, currentCSSZoom), real phone
- [x] Idle seat wording fixed in F4 (needs relay redeploy for the new wording)
- [x] F6 — Multi-client merged (fc466ff). Open: T11 measurements; public relay redeploy (user) for Join; leaked e2e dbus-daemon/wl-paste processes for the user to kill
- [x] F9 — Android app merged (92330df). APK ~/.cache/wado/wado-0.0.3-debug.apk. Untested on a real phone or live daemon.
- [x] F10 — Clipboard revamp merged (0f20a9c); daemon swapped (pid 544110). Open: pool origin label ("desktop" for a peer push); persist secret hashes across restart; phone checklist (Safari chip, Android app bridge)
- [x] wasm size: `--debug-symbols false` (1c8d3d2) — DWARF was ~13 of 15 MB; 2.6 MB → 0.93 MB gzipped (measured). Still open: first-paint on the phone.

## ▣ RUN — 2026-10-03, latency compass (Decision Log 2026-10-03)

- [x] Log the audio leg: abuf/atarget, video `jitterBufferMinimumDelay` (vmin = sync hold), input rt.
- [x] Daemon was already 10 ms Opus + 10 ms quantum, lowdelay, DTX off (libopus default). The
      "Low-latency audio" switch takes it to 5 ms + 5 ms and sets audio `jitterBufferTarget = 0`.
- [x] Latency line: audio + sync legs, "audio sync" culprit, input rt in the pill, A/B badge.
- [x] Found on the way: relay mode never opened `wado-motion` — touch moves rode the reliable
      channel (touch stalls on loss). Fixed both ends.
- [x] 3.1 Unreliable unordered data channel `wado-media` (server `wcmedia`, 18-byte chunk header,
      1100 B payload), reassembly + VideoDecoder + canvas. Codec string via isConfigSupported
      (Chrome rejects the SPS's own `avc1.640c2a`).
- [x] 3.2 AudioDecoder + AudioWorklet, audio clock is master, jitter-sized buffer capped 150 ms,
      late audio dropped/concealed. e2e: A/V offset < 50 ms.
- [x] v1 on the phone was worse (main-thread decode/paint → touch glitches, hw decoder 0.3–10 s).
- [x] v2: Worker + OffscreenCanvas + worker↔worklet MessageChannel; e2e main thread 5.8% busy.

## ▣ RUN — 2026-10-02, client redesign + daemon config + four features

- [x] C1 config.kdl · C2 hot reload, ui.kdl, `wado msg` · U1 landing + control centre ·
      U2 shell tabs · C3 binds, gestures, window rules · F1 clipboard · F2 notifications ·
      F3 screenshot/record · F4 audio.
- [x] Verified by machine: `scripts/shells-e2e.mjs` (17 checks: shells, reattach, ssh allow-list,
      config over relay) and `scripts/session-e2e.mjs` (clipboard both ways, notifications —
      real session, needs GPU). Audio is proven in headless Chrome: 3 B/pkt silent and
      136 B/pkt with a tone, at 100 pps. The UI was screenshotted at 390×844 and 1280×800,
      in light and dark schemes, against a live session.

## ▣ RUN OPEN — 2026-09-20, gamepad layout + who is connecting

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

## ▣ RUN CLOSED — 2026-09-19, drawer + windows fit + focus glow + pointer lock

- [x] **The 40-tile cap was hiding 32 of 72 apps.** `MAX_TILES` 40 → 400; it is a DOM guard,
      not curation.
- [x] **`NoDisplay`/`Hidden` entries are carried, not dropped.** New `AppEntry.hidden`;
      `parse_entry` reports instead of rejecting. 71 of 143 entries here.
- [x] **👁 and ▶ on the search row.** The eye lists everything installed (hidden-marked and
      iconless), persisted as `Settings::show_hidden`; ▶ and Enter run the box, resolving a
      typed name or command against the app list first (`ui/drawer/run.rs`).
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


## Run of 2026-09-19 — connection hardening (lane 2)

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
- [x] **Withdrawn: "the pinned UDP range is not taking effect".** Measured 2026-09-19 during a
      live session by mapping `/proc/<pid>/fd` socket inodes to `/proc/net/udp`: daemon-1 held
      50010/50013/50018/50024/50025/50031/50055/50089 — all inside its 50000-50099 slice. The
      pin works.
- [x] ~~The pinned UDP range is not taking effect.~~ `webrtc_settings.rs` sets
      `UDPNetwork::Ephemeral(50000-50400 sliced)`, but the daemon's live ICE sockets are
      `192.168.1.239:55333` and `172.16.0.2:59097`. No "pin failed" warning was logged. Not the
      cause of anything today — but a firewall rule opening the pinned range protects nothing.

## Run of 2026-09-14 — multi-device pool

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
- [x] **Stickiness IS deployed — entry below withdrawn.** Observed live `2026-09-19 15:48`: a
      browser disconnected and rejoined as `assignment="reclaimed"` onto its own instance
      (`29eb0c39`), windows intact. The Mac did the same onto `f49ba537` repeatedly. The
      `?instance=` marker is in the shipped wasm.

## Awaiting the user (blocked on a human, not on work)

- [x] **Track the planning docs** — `2026-11-09`: all agent files moved into `plan/` and tracked on
      the `plan` branch; `CLAUDE.md` rebuilt there. `WADO_PLAN.md` and `CHALLENGES.md` were lost.

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


## Next up

- [x] **Chrome takes the dmabuf path** — `AB24` under an AMD tiled modifier, confirmed live on
      the first session of `2f55d82`. R10 is closed.
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
- [x] **Dead exec path deleted** (`459dfca`) — 272 lines across four crates, after confirming
      nothing called `W.relayExec` and no UI read `term`/`term_input`/`term_busy`. Supersedes
      `e05aac6`, which had tied an exec child to its connection: there is no exec child now.
- [x] **Stall instrumentation uncensored** (`62aeaa9`). Every frame recorded; p50/p90/p99/max per
      300 frames. Unit-tested, including the case the threshold log could never show. A real
      stall cannot be forced, so the live path ships unverified.

## Wayland protocols — what is left

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


## `graceful` branch — merged to main 2026-09-13

- [x] **Session survives a disconnect.** The killer was `relay/src/signaling.rs` synthesizing
      `session_stop` on any socket close. Field-confirmed by the user: reconnect → streaming in
      8 s with the window intact, nothing pressed.
- [x] **The relay link is persistent.** `js/relay_link.js` — dials at page load, reconnects
      forever with backoff. A `wado.watching` crumb carries a session across a page reload.
- [x] **Bitrate and aspect ratio change on the fly.** `Reconfigure`, 6–50 ms, applications
      alive. **Apply** button next to Start/Stop.
- [x] Hardening pass: 9 error paths closed, each with a check that fails without it.
- [x] **`SentKbps` client half** — done and deployed. The verdict now splits three ways; the
      two bugs the new cases caught (`+null === 0`, and a harness that set the value before the
      reset wiped it) are written up in `gracefulness.md`.

## ▢ Frame-rate ↔ refresh-rate sync — asked `2026-09-13`

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
