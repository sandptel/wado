# wado — work tracker & handover

<!-- HUMAN NOTE: This file is STATE, not architecture. It answers three questions and
     nothing else: how much is done, what is next, and what must survive into the next
     session. Rationale goes in WADO_PLAN.md's Decision Log; rules go in CLAUDE.md.
     Supersedes the old TODOS.md. Keep the dashboard counts in sync when ticking items —
     they are the whole point of the file. -->

> **How to use this file.**
> 1. Read **Handover** first — it is the only part that goes stale in hours rather than weeks.
> 2. Update **Handover** before the session ends, even if nothing shipped.
> 3. Tick items in place with a date, and move the count in the dashboard.
> 4. New concerns get written down the moment they surface, in the right category, even
>    mid-task. The current session keeps its scope; the note survives.

**Categories** — urgency, not effort:

| | Meaning |
|---|---|
| **NOW** | active milestone; in flight or next up |
| **NECESSARY** | not deployable or not safe without it |
| **GOOD-TO-HAVE** | polish, performance, reach |
| **DEFERRED** | parked on purpose; revisit when its scope is active |
| **HYGIENE** | debt that slows every future session |

---

## Progress dashboard

| Category | Done | Open | Total |
|---|---|---|---|
| NOW (product track PV, P0…P4) | 4 | 39 | 43 |
| NOW (file manager F0…F5) | 7 | 4 | 11 |
| NOW (phone UX M-P) | 0 | 6 | 6 |
| NOW (client UX M-C1…M-C4) | 21 | 2 | 23 |
| NOW (M4 iteration 1) | 11 | 0 | 11 |
| PLANNED (macOS MAC0…MAC5) | 0 | 6 | 6 |
| NECESSARY | 3 | 7 | 10 |
| GOOD-TO-HAVE | 0 | 12 | 12 |
| DEFERRED | 3 | 7 | 10 |
| HYGIENE | 5 | 1 | 6 |
| Handover (verification + decisions) | 1 | 24 | 25 |
| **Total** | **55** | **108** | **163** |

Counts are mechanical — every item is a `- [ ]` / `- [x]` under its category heading, so they
can be re-derived rather than trusted. Re-derive them after any batch of edits; a dashboard
that disagrees with the list below is worse than no dashboard.

**Milestones:** M0 compositor ✓ · M1 ✓ · M2 input ✓ signed off · M3 video pipeline ✓ signed
off · **M4 internet/relay — iteration 1 done, iterations 2–4 open** · M5 per-client output,
M6 native client — not started. **Client UX track M-C1…M-C4 — designed `2026-09-11`, no code
yet**; it runs alongside M4 rather than inside it, since it touches the client, the protocol
and the compositor's command surface but nothing in the relay.

**Gate on M4 completion:** the relay cannot be deployed publicly until the three security
items in NECESSARY (confirmation gate, join rate-limit, TLS) are done. The Remote ID is ~30
bits and is currently the only secret.

---

## NOW — product track (PV, P0…P4), designed `2026-10-27` — see Decision Log

The plan to make wado usable end to end. Strictly in order; each phase closes with its
real-phone checklist ticked, never with "builds clean".

### Trackpad mode, plan 1 — built `2026-10-30`, awaiting the phone
Spec `docs/superpowers/specs/2026-10-29-trackpad-mode-design.md`. Grant first: `wado allow <phone> input`.
- [ ] 20 taps = 20 clicks; two-finger scroll never clicks; three-finger Hyprland swipe always switches.
- [ ] A resting thumb at the edge doesn't move the pointer.
- [ ] Dictate "Hello, café." — lands exactly. Super then 1 switches workspace. Locked Super + drag moves a window.
- [ ] Kill the browser mid-drag / mid-Super-lock: nothing stuck (fingers ≤ 400 ms, keys ≤ 5 s).
- [ ] Relayed (phone on mobile data, direct blocked): no catch-up bursts; record the pill's ms next to the LAN number in `plan/memory/latency/measurements.md`.
- [ ] Gboard: Backspace on an empty field reaches the computer (may report key "Unidentified").
- [ ] Plans 2 (actions, quick bar, settings) and 3 (phone gestures) after this checklist.

### Trackpad — later (scope-cut 2026-10-30)
- [ ] Key-combo buttons on the rail/quick bar (any shortcut as a button).
- [ ] Custom actions: commands in `config.kdl` (`host`) and created from the phone (`shells`).
- [ ] Phone checklist for plan 2: landscape held sideways with rotation lock on — pointer goes the
      right way; quick actions (workspace next/prev, fullscreen, play/pause, volume); each phone
      gesture once; a three-finger swipe still switches workspace; settings survive a reload.

### PV — phone verification sprint (before any new code)
- [ ] Walk every `Awaiting user verification` item in the Handover below on a real phone; tick it
      or file the bug. Includes F0 on a phone, files on a phone, seat UX, M-P S1–S7, M-C1…M-C4.
- [ ] Fix the stale e2e tests (`relay-seat-e2e`, `relay-link-check`) and the pool-e2e Join flake,
      so the suites are a trustworthy net for P0.

### P0 — connect & trust
- [ ] **Relay on a VPS**: dual-stack (IPv6 matters on this 464XLAT carrier), Caddy TLS, DNS-only
      record (no Cloudflare proxy — its idle timeout dropped sockets before), `/health` uptime
      check alerting the user. Runbook: `deploy/relay/README.md`.
      *`2026-10-28`: **live** at `wss://wado-relay.indiasouthcentral.cloudapp.azure.com` — Azure for
      Students, RG `wado-relay-rg` (`indiasouthcentral`: the subscription's policy forbids
      `centralindia`), B1s Ubuntu 24.04, ssh `wado@<host>`, LE cert via Caddy 2.6.2, $10/mo budget
      alert. HTTPS + WSS verified from here. **Still open:** uptime monitor; inbound IPv6 unverified
      (this host had no v6 route) and the Azure name has no AAAA — add one with the real domain.*
- [x] `DEFAULT_RELAY` → the new `wss://` URL; delete `server::selfhost` (cloudflared + local
      relay bootstrap) ✓ `2026-10-28` (`832ac76`). Daemon registers with no config; client migrates
      saved trycloudflare URLs. **Left on purpose:** `rig.sh` still runs its own local relay + tunnel
      for dev, and the repo flake still ships cloudflared for it — delete both when rig.sh moves over.
- [ ] **QR / pairing-code only**: delete first-device-wins in `gate`; codes single-use, 10 min.
- [x] Daemon's `nat.rs` verdict → client (`host_net_request`/`host_net`, forwarded-only) ✓ `2026-10-28`;
      the badge's relayed detail names a symmetric NAT. Verified headless (`consistent` arrives).
- [x] **Online vs connected** states + persistent link badge ✓ `2026-10-28` (`js/link_state.js`,
      `ui/link_badge.rs`, `link.rs`): offline / computer offline / waiting / connected / connecting /
      relayed — limited / reconnecting / direct·lan|internet|turn; dot-only during a direct session.
      Daemon sees each device (`device_status` → `crate::links` → `wado tui` "now" column, `wado`,
      log on change). Verified headless against the public relay; **awaiting a phone.**
- [ ] **Fallback lane**: per-seat 64 KB/s cap in the relay; client hints where it hurts + *Retry direct*.
- [ ] **Fast reconnect** — *built `2026-10-28`* (`js/netwatch.js`, `W.relayKick`, own-seat takeover —
      Decision Log). Simulated (`online` event, nothing actually changed): picture back in 1.75–1.9 s
      on a started session, 0.98 s on a joined one; the real Wi-Fi→5G number is still unmeasured —
      read `browser: network: reconnected in N ms` in the daemon log after a phone handoff. Was: on network change (online / `connection.change` / ICE disconnected), session
      untouched, "Reconnecting…" overlay. **Measure** Wi-Fi→5G screen-back; target < 2 s. Spike
      ICE restart on webrtc-rs 0.17 only if missed.
- [x] Relay-down message: "wado's relay is unreachable — not your computer" ✓ `2026-10-28` (badge
      detail, `offline` + `relay_unreachable`; `no_network` says it is this device).
- [ ] **P0 phone checklist**: pair by QR on mobile data; badge correct on LAN, 5G, and behind a VPN;
      Wi-Fi→5G handoff number recorded; a Remote ID alone cannot get past "waiting".

### P1 — shells, Claude, service worker
- [ ] **Service worker** + PWA manifest; serves a **pinned client**, activates only updates signed by
      the offline release key; version shown. Release-signing step documented.
- [ ] **Passkey step-up for shells**: register at pairing, ES256 verify, 15 min idle / background re-lock,
      no authenticator ⇒ no shells.
- [ ] Shell audit log per device; TUI "who is attached" + kick.
- [ ] Route exec'd console-shell processes into `app_processes` (the open process-lifetime gap).
- [ ] `wado claude hooks install|uninstall` (shows the diff) + `wado claude-event` → daemon.
- [ ] Claude status cards (shells grant, no passkey, fallback-lane capable); tap → shell.
- [ ] "New Claude session": pick a project dir → `tmux new -d -s wado-<project> claude`, shell attaches;
      no tmux ⇒ plain wado shell.
- [ ] **Web Push** (VAPID, encrypted payloads), per-kind per-device toggles; default on: Claude needs
      permission. iOS: install-to-home-screen prompt.
- [ ] **P1 phone checklist**: passkey on Android + iPhone; Claude permission prompt buzzes a locked
      phone and the tap lands in the right shell; a Pages-pushed unsigned build is refused.

### P2 — clipboard & files
- [ ] Clipboard: drop `x-kde-passwordManagerHint: secret` copies everywhere.
- [ ] Clipboard: RAM-only last-50 history when `cliphist` is absent.
- [ ] Clipboard: auto-offer on focus (Chrome), tap on Safari.
- [ ] Files: native physics — virtualised momentum list, edge-swipe back, pull-to-refresh, long-press
      select, haptics, prefetch; **measure** cached folder paint < 150 ms.
- [ ] Files: Safari downloads streamed through the service worker.
- [ ] Files: Android **Share → wado** (Web Share Target).
- [ ] Files: listings + files ≤ 1 MB on the fallback lane, with the limited-lane hint.
- [ ] **P2 phone checklist**: 10k-entry folder scrolls at 60 fps; iPhone download lands; share a photo
      from the Android gallery; copy a KeePassXC password — it never appears.

### P3 — gaming
- [ ] Physical controllers: Gamepad API → own unreliable newest-wins channel → uinput Xbox pad.
- [ ] `wado doctor` + first-run encoder self-test (tier, max sustained res/fps, plain-words verdict),
      cached per GPU + driver; `--report` for beta testers. Sets defaults and the resolution menu.
- [ ] `nvenc-cpu` tier (ffmpeg `h264_nvenc`, open-and-cache probe), verified on the user's NVIDIA box.
- [ ] Spike: CUDA-GL interop vs Vulkan Video zero-copy, measured on NVIDIA; decide, then build.
- [ ] Intel VA-API verified on the user's Intel machine.
- [ ] Software tier release fps measured on a CPU-only cloud VM (the open known-unknown).
- [ ] Steam open on the host: detect, offer to move it into the session.
- [ ] **M5 — per-device `Output` at connect** (invariant 8).
- [ ] **P3 checklist**: a real game played with a Bluetooth pad on the phone, on AMD, Intel and NVIDIA.

### P4 — packaging
- [ ] CI-built static ffmpeg 8 (vaapi + nvenc + needed parsers only); Mesa/libva/driver from host.
- [ ] `.deb`, `.rpm`, AUR PKGBUILD, Nix — each with the uinput udev rule and a `systemd --user` unit.
- [ ] Fresh-machine install test per package: install → QR → session, no manual steps.

### Later (recorded, not scheduled)
- [ ] Android app (M6): **phone → computer notifications**; hardware decode.
- [ ] TURN, if the fallback lane proves too limiting in practice (hook already in `ice.rs`).
- [ ] Rootless Xwayland + XWM, on a demonstrated game/launcher breakage.

---

## Handover — read first

*`2026-10-28` — public relay live and default; P0 link badge, device status, NAT verdict and fast
reconnect built and verified headless — **the user is testing on a phone next**. PV was skipped on
the user's call to do this first. Open in P0: uptime monitor, fallback-lane cap, QR-only pairing,
real handoff measurement. Dev-shell EGL/GBM pin added (see `plan/memory/shared/environment.md`).*

*Previously `2026-10-27` — product plan decided (Decision Log).*

*Previously `2026-09-29` — hygiene pass done (all five HYGIENE items); docs audit corrected stale NECESSARY/DEFERRED items. Dashboard recounted mechanically.*

### Awaiting user verification — the `2026-09-20` gamepad / fullscreen work

- [ ] Edit mode: every control drags, − / + resize the selected one, ⟲ and *Reset the layout*
      both restore. Layout survives a reload and a session at another resolution.
- [ ] Press feedback is visible past a fingertip on a real phone (grow 1.16× + ring).
- [ ] A dragged control still animates on press — `scale` must not be cancelled by the inline
      positioning `transform`.

Builds clean (131 tests, `dx build` green, the release daemon pool is swapped onto it), but
every one of these is a browser-or-phone judgement and none has been made.

- [ ] **The pad is usable on a phone in landscape.** Size/opacity/edge sliders cover a range;
      what they do not cover is whether the default 26 px unit puts the sticks where thumbs
      actually rest. This is the one thing only a phone can answer.
- [ ] **"Keys" mode drives a game.** Left stick walks, right stick looks, shoulders click.
      Mouse-look speed is a fixed `LOOK_SPEED = 14` px/frame in `js/gamepad.js` with no slider —
      if it is wrong, that is the number to move, and it probably wants a setting.
- [ ] **"Real controller" mode, once `/dev/uinput` is permitted.** BLOCKED on a NixOS change:
      `hardware.uinput.enable = true` plus this user in the `uinput` group, then a re-login. See
      `plan/memory/shared/environment.md`. Until then the mode logs a permission error and does
      nothing, by design. Check with `evtest` or `SDL_GAMECONTROLLER_*` that the pad enumerates
      as an Xbox 360 controller and that A is A.
- [ ] **A game actually goes fullscreen** inside the session, fills the output, loses the focus
      ring, and comes back to where it was. Untested against a real client — nothing in the tree
      exercises `set_fullscreen`.
- [ ] **A phone session comes up landscape** and the resolution dropdown offers e.g. 1600×720
      rather than 720×1600. Desktops and tablets must be unaffected.
- [ ] **The ⊟ window-action popup** opens above the bar, does not clip, and closes after acting.

### Awaiting user verification (blocking nothing, but unconfirmed)

These are implemented and build clean, but only a human with a browser can confirm them.
Nothing here should be described as working until it is ticked.

- [ ] **Playout-delay fix actually takes effect** — Debug → ping shows a new `N ms buf`
      figure. Under ~30 ms means fixed; still 100 ms+ means the hint was ignored (only
      Chromium implements it; Firefox has neither knob) and the lag is elsewhere.
- [ ] **Window dragging no longer lags** — the uncoalesced drag-motion flood is fixed, but
      "feels right" is a human judgement.
- [ ] **Two input channels behave under real input** — motion on the zero-retransmit
      channel, buttons/keys on the reliable one. The motion channel is now **ordered**:
      absolute positions cannot tolerate reordering, and unordered delivery would have
      needed a sequence number plus a server-side newest-wins filter to be correct.
- [ ] **Latency breakdown reads sensibly** — Debug → "Latency breakdown (per stage)".
- [ ] **Browser video is artefact-free under heavy motion** on the zero-copy tier.
- [ ] **Live encoder-downgrade marker** — never verified end to end, and see the NECESSARY
      item about it: the SSE half does not exist.
- [ ] **Browser-via-relay video** sign-off.

### Process lifetime — one gap left `2026-09-11`

Applications launched into a session now die with it: each is its own process group, killed
as a group, and SIGINT/SIGTERM runs `stop_session` instead of skipping it. See
`compositor/src/proc.rs`.

- [ ] **Console-shell processes are not tied to the session.** `server/src/exec.rs` spawns
      into the daemon's own process group and never registers the child in
      `app_processes`, so a GUI application started from the console shell still outlives
      its session. Deliberately not "fixed" alongside the launcher path: giving those their
      own group would stop a terminal Ctrl-C from reaching them, which makes the leak worse,
      and the documented behaviour is that a GUI app started there *should* keep running.
      The right fix routes exec'd processes into `app_processes`, and the PTY work is the
      natural moment — it rewrites that path anyway.

### Known-unknowns worth one measurement each

- [ ] **Software tier throughput in a release build.** It manages only ~13.6 fps against a
      60 fps target in a debug build (1280×720), which is expected — unoptimised readback
      and colour convert — but the release number is unknown and decides whether the x264
      fallback is actually usable.
- [x] **Where the remaining perceived latency lives.** Not the server. Measured at
      1280x720/**120 fps**, release build, `vaapi-dmabuf`: a steady 120.0 fps, zero drops,
      capture 0.2 ms + encode 3.4 ms + queue 0.03 ms. The render loop has ~5 ms of slack
      per frame at 120. Everything left is client- or network-side. ✓ `2026-09-11`
- [ ] **Does the 20 ms playout floor fix the 120 fps judder?** `jitterBufferTarget` was 0,
      which removed the very buffer that absorbs arrival jitter — and the cost scales with
      fps, since the same few ms of wobble is a fifth of a 60 fps frame but half of a
      120 fps one. Now 20 ms, tunable live via `W.playoutMs` in the console. Sweep it
      (0 / 10 / 20 / 40) and record where smooth starts.
- [ ] **If judder survives the floor, suspect `requestAnimationFrame`.** The input
      coalescer flushes on rAF, so input send cadence is tied to the browser's paint
      cadence — which gets less regular as compositing load rises. Untested; the server
      measurement does not implicate it, so it is a hypothesis, not a diagnosis. Swapping
      to a fixed interval is the experiment.

### Open decisions needing a human

- [ ] **Smithay: pin the revision in the manifest, or migrate?** The revision is currently
      held in the lockfile only, so the next dependency update re-breaks the build (38 errors
      last time — pointer/touch event `time` became `InputTime`). Either pin it in
      `crates/compositor/Cargo.toml` or do the migration as its own change.
- [x] **TLS approach for the relay** — Caddy, decided `2026-10-02`.
- [ ] **macOS backend: which Mac, and how is it driven?** MAC1 onward needs an Apple-silicon Mac
      (macOS version?) that builds can run on, by ssh from here or by hand. Nix darwin dev shell or
      Homebrew for ffmpeg/x264?
- [ ] **macOS backend: what does `Stop` do to apps launched into the session?** They are the
      user's own Mac apps, not session-private Wayland clients: quit them (as Linux does) or leave
      them running and let their windows fall back to the real display?

### The `2026-09-11` client-UX track: **all four milestones are written, none are confirmed**

M-C1…M-C4 are implemented and committed (four commits, `dd1ba02`…`ea7ccfc`). Rationale and
rejected alternatives are in `WADO_PLAN.md`'s Decision Log; per-milestone notes and thresholds
are in the **NOW — client UX track** section below.

**What was actually verified, and what was not.** The workspace builds, `dx build` produces a
WASM bundle, and 15 unit tests pass. `GET /apps` and `POST /session/control` were exercised
against a running server (71 apps found; unknown verb → 400; no-session verb → 200 plus a
compositor warning; the removed `/session/launch` → 404). **Nothing has been opened in a
browser, and nothing has been touched on a phone.** Every `**Threshold:**` line is deliberately
not a checkbox because it is exactly that gate. The largest unverified surfaces, in the order
worth checking:

1. **Settings survive a reload.** The persist effect is gated on a `loaded` flag; without it
   the first effect ran before the async load and wrote defaults over the saved blob. The gate
   is right by construction and completely unexercised.
2. **Debug off actually stops the timers.** The whole point of the registry. Check that the
   input data channel goes quiet and `/timing` stops being fetched.
3. **Relay mode**, for all of it. Relay shares no code with the HTTP path that was exercised —
   window actions, the app list, and the now-wired latency breakdown are all untested there.
4. **The phone**, for all of M-C2: sheet, safe areas, orientation lock, wake lock.

**Two long-standing bugs were fixed in M-C1** and are worth knowing about when reading old
measurements: `js/webrtc.js` started the latency probes unconditionally (a 2 Hz ping on the
input data channel with Debug off), and `js/relay.js` never started them at all, so **every
relay-mode latency breakdown to date was absent rather than good**.

**Still open from this track:** on-screen-keyboard handling (M-C2), and narrowing the
free-form launch command once the auth gate lands (M-C4).

**One pre-existing bug this track widened, deliberately not fixed here.** `<details open=…>`
is a controlled attribute on an element the user can also toggle natively. Once someone closes
the log panel by hand, Dioxus's VDOM still holds `open: true`, so `startFailed`/`giveup`
setting `logs_open` to true diffs true-against-true and writes nothing — the logs do not open,
which defeats the whole point of opening them on failure. The new settings `Group`s use the
same pattern, so a collapsed Session group re-expands when its `note` changes at session start.
The fix is an `ontoggle` handler syncing the signal back from the DOM.

---

## NOW — phone UX track (M-P), designed `2026-09-29` — see Decision Log

- [ ] **S1 — translated touch.** *Built `2026-09-29` (`input_tap.js`, `scripts/touch-check.mjs`); awaiting a phone.* tap/double-tap(snapped)/long-press→right-click/1-finger kinetic scroll/2-finger press-drag. Client JS.
- [ ] **S2 — window list + scrolling strip + bottom bar.** *S2a–c built `2026-09-29` (`window_list.rs`, `strip.rs`, `switcher.js` — the floating spring dial replaced the bottom bar at the user's request); awaiting a phone. S2d (thumbnails + overview) open.* Protocol `WindowList`/per-id actions/thumbnails; strip layout (portrait full, landscape ½|full); dialogs float over parent; client-animated swipe; icon bar with tint.
- [ ] **S3 — won't-fit apps.** *Built `2026-09-29` as one mechanism (see Decision Log): per-column factor, crisp, verified on a real frame via `examples/strip_check.rs`. Awaiting a phone for input through a shrunk column.*
- [ ] **S5 — AT-SPI in the session.** *Built `2026-09-30`: per-session a11y bus, zbus query (8–17 ms live). See Decision Log.*
- [ ] **S4/S6 — lens.** *Built `2026-09-30`: magnetic snap, tap-to-lens, pixel fallback, two-finger lens. Awaiting a phone.*
- [ ] **S7 — menus as a native bottom sheet** *Built `2026-09-30`, verified server-side on live nautilus. Awaiting a phone.*

## NOW — client UX track (M-C1…M-C4), designed `2026-09-11`

Design settled in a grilling session; rationale and every rejected alternative are in
`WADO_PLAN.md`'s Decision Log under the `2026-09-11` client-UX entry. **Client-first and
in order** — each confirmed in a real browser before the next starts. No code written yet.

### M-C1 — client restructure
- [x] Split `crates/client/src/main.rs` (673 lines) into `state.rs` / `bridge.rs` / `cfg.rs`
      / `ui/{connection,session,live,debug,stage}.rs`; `main.rs` becomes a thin dispatcher.
- [x] Regroup settings by **when they apply**: Connection / Session (disabled while running)
      / Live / Debug. Delete the four hand-appended `"(applies at Start)"` label suffixes.
- [x] Debug registry: `{ id, label, default, js: Option<&str> }` + a master switch defaulting
      ON with today's per-item defaults (fps/ping on, touches/latency off).
- [x] **Fix:** `js/webrtc.js:35` starts `W.latency` unconditionally — a 2 Hz ping on the input
      data channel and a 1 Hz `/timing` fetch run even with Debug off. Gate on the toggle.
- [x] **Fix:** `js/relay.js:168` starts stats but never starts latency — relay mode has no
      breakdown at all.
- [x] Persist everything in one `localStorage` key (`wado.settings`), restored on mount,
      unknown keys ignored on read. Lives in the JS bridge, not a new `web-sys` dep.
- [x] Split `assets/main.css` into `base` / `layout` / `stage`; tokens become base16
      (`--base00`…`--base0F`).
- [x] Base16 theming: 3–4 bundled schemes, a picker persisted in the same blob, and a paste
      box scanning `base0X`-to-hex pairs (~10 lines, no YAML dep). Stage backdrop forced dark
      regardless of scheme.
- [x] Motion: six `transform`/`opacity`-only transitions on one easing token; telemetry values
      never animate; `prefers-reduced-motion` disables all of it.
- **Threshold — NOT yet confirmed:** `dx build` is green and the 8 unit tests pass, but the
      threshold is browser-only. Still to check by hand: every control works as before,
      settings survive a reload, Debug off demonstrably stops the ping/`/timing` timers, and
      relay mode now shows a latency breakdown.
- Implemented `2026-09-11`. Two refinements against the design: **Appearance became its own
      group** (theme sits badly next to "launch a command", and the paste box is tall enough to
      bury its neighbours), and the **bundled palettes live in `js/theme.js`, not Rust** — the
      saved scheme has to be applied during the bridge's synchronous head or the page flashes
      the default palette first. Rust still owns parsing, per the original decision.
- One bug found and fixed while building, not in the original list: `use_effect` runs before
      the bridge's async load, so the persist effect wrote defaults over the saved blob on
      every load and **nothing would ever have survived a reload**. Gated on a `loaded` flag.

### M-C2 — mobile
- [x] One settings component, two presentations: docked column ≥720px, slide-over sheet below.
- [x] `env(safe-area-inset-*)`, `100vh` → `100dvh`, 44px minimum touch targets.
- [x] Fullscreen API button; `screen.orientation.lock()` matched to the session aspect.
- [x] `navigator.wakeLock` so the phone does not sleep mid-session.
- [x] `overscroll-behavior: none` + `touch-action` so a drag cannot pull-to-refresh and kill
      the session.
- [ ] **Deferred to its own pass, still open:** on-screen-keyboard handling (VirtualKeyboard
      / `visualViewport`) — browser behaviour is too inconsistent to bundle here.
- **Threshold — NOT yet confirmed:** `dx build` green; the threshold is a real phone. Still
      to check by hand: startable and controllable one-handed, portrait and landscape, video
      not letterboxed by chrome, and the sheet clear of the home indicator.
- Implemented `2026-09-11`. Two things the design did not anticipate: dx's generated
      `index.html` ships a viewport meta **without** `viewport-fit=cover`, so every
      `env(safe-area-inset-*)` would have resolved to zero — a custom `crates/client/index.html`
      now supplies it. And fullscreen targets the whole shell rather than the stage, because
      fullscreening only the video puts the settings sheet out of reach until you leave it.
- The control bar landed here rather than in M-C3: the sheet needs something to open it, and
      building a throwaway toggle first would mean writing the bar twice. M-C3 adds the four
      window actions to the bar that already exists.

### M-C3 — remote window control
- [x] `WindowAction { Maximize, Minimize, Close, CycleFocus }` in `wado-protocol`, acting on
      the **focused** window only (no window list to track or push).
- [x] Compositor handlers. **Minimize means lower-to-back, not unmap** — with no window list
      an unmapped window is unreachable forever; cycle-focus must always be able to reach it.
- [x] Collapse control onto one route: `POST /session/control` taking
      `SessionControl { Launch{command}, Window(WindowAction) }`, plus one `session_control`
      relay message. **Corrected during implementation:** only the HTTP side consolidates.
      `RelayMsg` is a flat, forward-verbatim namespace where one variant per action already
      costs the relay nothing, so `SessionLaunch` stays and `SessionWindow` joins it as a peer.
      Nesting there would have meant two shapes for one layer. The `/session/launch` *route*
      is removed; the relay message of that name is not.
- [x] Auto-hiding edge bar over the video (fades ~2 s, returns on tap-near-edge), carrying the
      four actions plus the ⚙ that opens settings.
- **Threshold — NOT yet confirmed:** workspace builds clean, 11 tests pass (3 of them pin the
      exact JSON the browser hand-writes, since the client shares no types with the server).
      Still to check by hand: all four actions from a phone in **both** direct and relay mode,
      and that a minimized window is recoverable by cycling.
- Implemented `2026-09-11`. **A pre-existing bug fixed on the way:** `placement.rs` applied
      `Placement::Maximized` by setting the size but never setting `xdg_toplevel::State::
      Maximized`, so an app was resized without being told it was maximized — GTK header bars
      and the like drew wrong, and the new maximize toggle had nothing to read back, which
      would have made the first press on a start-maximized window maximize it again.
- Minimize focuses the new top window explicitly: `lower_element` carries no activate flag, so
      without that the lowered window would keep the keyboard — invisible and still typing
      into, which is the exact unreachable state the lower-don't-unmap design exists to avoid.

### M-C4 — app launcher
- [x] Server scans `/usr/share/applications` + `~/.local/share/applications`, serves
      `GET /apps` (`name` + `exec`, `%f`/`%u` field codes stripped). `.desktop` is an INI —
      ~30 lines, no new dependency.
- [x] Relay `apps` request/response message; relay mode has no HTTP path to the server.
- [x] Filter-as-you-type picker, free-text box kept as an escape hatch.
- [ ] **Revisit when the auth gate lands (still open):** `CLAUDE.md` records that the control plane binds
      localhost *because* the launch command is free-form RCE. A picker is the mechanism that
      could shrink that surface to a known set — not acted on in this milestone.
- **Threshold — partly confirmed, by running the server.** `GET /apps` returned **71** real
      applications with `Exec=` field codes stripped; `POST /session/control {"Window":"explode"}`
      → 400 naming the four valid verbs; `{"Window":"maximize"}` with no session → 200 and the
      compositor logged `window action ignored — no active session` rather than failing
      silently; the old `POST /session/launch` → 404. Still to check by hand: the picker and
      the four window buttons **from a phone**, and the whole path **in relay mode**, which
      shares no code with the HTTP path that was just exercised.
- Implemented `2026-09-11`. Scanning reads `XDG_DATA_HOME`/`XDG_DATA_DIRS` rather than
      hard-coding `/usr/share`: on NixOS, where this is developed, a hard-coded list finds
      nothing. `crates/server/examples/apps_probe.rs` prints what the scan sees — the unit
      tests cover the `.desktop` parser but cannot cover the scan, which is exactly the part
      that differs between a dev shell and a target machine.
- One input, not two: the command box *is* the filter. A separate search field would mean two
      places to look and a rule about which one wins.

---

## NOW — M4 iteration 1 (relay + Remote ID) — complete ✓ `2026-06-10`

Identity is a single **9-digit Remote ID**, both address and access token; no password. It
resolves from env → persisted config → generated. The relay is a dumb verbatim forwarder
once the handshake is done.

- [x] `crates/relay` standalone binary — one file per job (config/registry/room/signaling/error)
- [x] `wado-protocol::relay` — flat serde-tagged message enum + ID normalise/display helpers
- [x] Server registry keyed by normalised Remote ID; duplicate register rejected
- [x] Room store, one room per Remote ID, destroyed when either side closes
- [x] `/register` (server) + `/join/:remote_id` (client) — the join path *is* the auth
- [x] Server relay client: forwarded session control + SDP, auto-reconnect with backoff
- [x] Remote ID resolution + persistence
- [x] Client relay-mode UI — relay URL + Remote ID only
- [x] STUN configured on both direct and relay paths
- [x] Relay in workspace default-members (no GPU deps, builds everywhere)
- [x] `GET /health`

---

## NOW — relay finalisation (seat model), designed `2026-10-02` — see Decision Log

Deploy once, survive daemon majors. Each step ships alone with its check.

- [x] 1. `relay_wire` v1 split; `v` + `caps[]` in handshake — ✓ `ef15b15`, unit tests + live old-frame check against the new relay
- [x] 2. `instance_key` + `boot_id` — live-checked: relay restart → `reclaimed` same instance + boot_id; duplicate key refused; daemon death closes the client link. (`client_key` moved to step 4, where held seats need it.)
- [x] 3. Parked joins (`waiting`) — e2e ✓
- [x] 4. Held seats (`client_key`), one-tap takeover (gated by `peer_check`), 30 min grace — e2e ✓. `peer_away`/`peer_back` dropped: a daemon drop parks the client instead
- [x] 5. Two-way liveness, backoff cap 5 s, exact `pong` filter — e2e ✓ (SIGSTOP daemon → client released < 56 s)
- [x] 6. Approval gate on a connected device (files in the shared config dir; first device trusted) — e2e ✓
- [x] 8. `leave` (close code 4001): a client that is done frees its seat instead of holding it 30 min — e2e ✓. Also fixed the 4 stale `relay-link-check` failures (harness lacked the `W.dial` stub, not a client bug). Approval sheet rendered headless at 390 px: correct.
- [x] 7. Per-IP rate limit (`--trust-proxy` behind a proxy) + `deploy/relay/` Caddyfile and systemd unit — e2e ✓
- [ ] **Deploy the relay to a stable host** — needs a host and a domain (the open decision below). Then point `DEFAULT_RELAY` at it.
- [ ] **Verify the seat UX on a real phone**: the waiting text, "Use it here", the approval sheet, the 30 min resume. Only the protocol is tested; nothing has been looked at in a browser.

Relay host: VPS (Debian/Ubuntu) + domain, decided `2026-10-13`; coturn deferred. (Resolved `2026-10-02`: hold 30 min,
`instance_key` = `WADO_INSTANCE`.)

## NOW — file manager (F0…F5), designed `2026-10-04` — see Decision Log

F0 closes a hole that exists today without files: the relay can read and inject every
daemon↔client message, `SessionLaunch` and shells included. Each stage ships alone.

- [x] **F0 — end-to-end envelope over the relay.** ✓ `2026-10-04` built and verified in desktop
      Chrome (pool/session/shells e2e + hostile-relay cases). SRI instead of CSP (Decision Log).
- [ ] **F0 on a real phone**: the link comes up ("secure link up" in the daemon log), and a QR
      re-pair pins it (`trusted_clients` line ends in `1`). Needs Chrome 137+ on Android.
- [ ] **Pre-existing, found during F0**: `relay-seat-e2e` hold/takeover (8 checks) assume a held
      seat, but since `2026-10-03` an idle daemon frees it; `relay-link-check` cold-load resume
      (4 checks). Both fail identically on `83dc68f`. The tests need updating, not the code.
- [x] **F1 — browse.** ✓ `2026-10-04` — `files {}` config, `openat2` scope + denylist, `files=`
      grants + `wado files`, second peer connection, list/grid window in both layouts, entry points.
- [x] **F2 — download.** ✓ `2026-10-04` — queue (2), pause/resume, speed + ETA, pill, OPFS → Downloads
      (no service worker: Decision Log), SHA-256 check, folder zips, pacing while video is live.
- [x] **F3 — upload.** ✓ `2026-10-04` — `.wado-part` + hash + rename, resume, folders, clash sheet, disk check.
- [x] **F4 — file ops.** ✓ `2026-10-04` — new folder, rename, trash (freedesktop), move, copy.
- [x] **F5 — quick access.** ✓ `2026-10-04` — GTK bookmarks + XDG pins (pin/unpin from the device),
      recents, thumbnails, `files.log` audit, toasts on the pool's other devices.
- [x] **F1–F5 verified** in desktop Chrome: `scripts/files-e2e.mjs` 57/57, unit tests.
- [x] **File viewer, tier 1** ✓ `2026-10-05` — images (gallery, swipe/arrows, double-tap zoom), video,
      audio, PDF (desktop inline; phone hands to an app), text/code/JSON/CSV/logs (2 MiB cap),
      all browser-native, bytes hash-checked. HTML/SVG never rendered as documents (see files_view.js).
- [x] **Streaming + VLC-style player + photo zoom + UI revision** ✓ `2026-10-05` — any format via ffmpeg
      → fMP4 → MSE, seek, captions (embedded / beside / from device), audio tracks, VLC keys,
      resume position; Panzoom; overview, search, categories, grid everywhere, open-on-computer,
      share, properties, Trash restore. files-e2e all green; pool/shells unchanged.
- [ ] **Viewer, later** (in rough order of usefulness):
  - [ ] **HEIC / RAW photos**: convert on the host (ffmpeg / gdk-pixbuf) for viewing.
  - [ ] **Image-based subtitles** (PGS, VobSub): burn in during the transcode.
  - [ ] **Hardware transcode** (VA-API) when CPU is the limit; quality presets on a slow link.
  - [ ] **PDF on phones** inline: pdf.js (SRI-pinned).
  - [ ] **Markdown rendered**, code highlighted, CSV as a table; **edit and save** text files.
  - [ ] **Office documents** → PDF via LibreOffice if installed; **archives** listed and opened.
  - [ ] **Photo extras**: rotate-and-save, slideshow (EXIF details ✓ `2026-10-05`); **ebooks** (epub).
  - [ ] **Search index** (tracker/locate) if bounded walks prove too slow on big trees.
- [ ] **Files on a real phone**: re-pair by QR, `wado files grant <phone> rw`, browse, download a
      large file (check it lands in Downloads), upload a photo, long-press select. Watch for: OPFS
      `createWritable` (Chrome Android 121+; Safari lacks it — downloads there fail with a message).
- [ ] **pool-e2e Join flake persists** (~1 in 6 runs since F0: "Join session takes it back" fails and
      the session checks cascade). Not files-related; diagnose with E2E_LOG=1 on a failing run.
- [ ] **Files during a live session**: a big download while streaming — does the cap hold the
      video clean (`files.rate-with-video`)? Measure, don't assume.

## PLANNED — macOS host backend (MAC0…MAC5), designed `2026-10-17` — see Decision Log

Plan and invariant mapping: `WADO_PLAN.md` § Milestone MAC. Each step waits on the previous one's
threshold. MAC0 is Linux-only and can be done without a Mac.

- [ ] **MAC0** — boundary types into a Smithay-free `wado-backend` crate; `build` returns a `Runner`
- [ ] **MAC1** — server compiles on macOS (Linux-only deps/modules gated), macOS CI `cargo check`
- [ ] **MAC2** — main-display SCK capture → x264, CGEvent input, evdev→kVK table; taps land right
- [ ] **MAC3** — per-device `CGVirtualDisplay` with main-display fallback; `Launch` moves windows onto it
- [ ] **MAC4** — VideoToolbox H.264 zero-copy, open-and-cache probe, downgrade to x264; measure
- [ ] **MAC5** — clipboard, text-input watch, audio, window actions, shedding

## NECESSARY

Security first — the first three are the public-deployment gate.

- [x] **Join rate-limiting on the relay.** ✓ `2026-10-02` (`ratelimit.rs`; `--trust-proxy` behind a tunnel). A token bucket per source IP. The Remote ID is
      ~30 bits and is now the only secret, so unthrottled joins make it brute-forceable
      online. Highest-value security item. Note: behind a Cloudflare tunnel every join
      arrives from 127.0.0.1, so a per-source-IP bucket buckets the whole internet into
      one — the peer address has to come from the forwarded header in that deployment.
- [x] **Confirmation gate on incoming peers.** ✓ `2026-10-02` (`gate.rs`, `peer_accept`/`peer_reject`). The relay holds a join until the server
      approves or denies, and only then accepts. Hook sites are already doc-commented in the
      relay's join loop and the protocol. **Approve on an already-connected device** (decided
      `2026-10-02`, supersedes "CLI prompt first"). With no device connected: known `client_key`s get in, unknown ones wait for one.
- [x] **TLS on the relay.** ✓ `2026-10-02`, decided: Caddy in front (`deploy/relay/Caddyfile`). Not deployed yet. Mandatory before any public deployment. Decide: reverse-proxy
      termination (simplest, no Rust TLS code) or native rustls (self-contained binary).
- [x] **`PeerDisconnected` relay message.** ✓ found done in audit `2026-09-29` (`signaling.rs` sends, `relay_client.rs` handles). Room cleanup currently synthesizes a raw
      `{"type":"session_stop"}` into the server's inbox so a vanished viewer cannot leak
      `session_active` (commit `e1cb52b`). It works, but it makes the relay author a
      message rather than forward one verbatim, which is the one thing the relay is not
      supposed to do. A real variant + match arm is the clean version.
- [ ] **Auth gate on the server's control endpoints.** Session start/stop/launch and the
      offer endpoint are unauthenticated; this gate is what permits binding beyond
      localhost, because the launch command is free-form RCE. A startup-configured bearer
      token is enough to begin.
- [ ] **Per-server viewer cap**, default one, configurable. *Audit `2026-09-29`: relay mode already caps one viewer per daemon (pool size = `WADO_INSTANCES`); direct mode still replaces the old viewer.*
- [ ] **Live session events over SSE.** There is no named-event mechanism at all: the event
      stream carries log lines only, and the encoder-downgrade marker that was believed
      wired does not exist. Needed for any server→client push (downgrade notices, approval
      prompts). Per-stage timings currently poll a plain endpoint because of this.
- [x] **CI/CD for the WASM client** ✓ found done in audit `2026-09-29` (`.github/workflows/pages.yml`). → GitHub Pages. Needs the wasm target and `dx` in CI,
      and a base-path check for Pages.
- [x] **Runtime-configurable relay URL in the client** ✓ found done in audit `2026-09-29` (editable + persisted; `DEFAULT_RELAY` staleness tracked in `plan/TODO.md`)., so one built artifact works against
      any relay instead of needing a rebuild.
- [ ] **Structured logging in the relay** — registrations, joins, auth failures, room
      lifecycle; JSON in production.

---

## GOOD-TO-HAVE

- [ ] **Pocket Console phase** (Decision Log `2026-10-29`, `DESIGN.md` North Star). A session opens
      with the picture inset at its true aspect inside a console shell, controls around it; full
      screen grows the picture and the same controls morph into the floating cockpit. Surround
      contents undecided (window controls, input controls, status + tiles are the candidates) —
      shape it (`/impeccable shape`) before any code.
- [ ] **Damage-gated capture/encode.** Today every render tick does a full capture + full
      encode + send, whatever changed. The `OutputDamageTracker` is already wired but
      passed `age: 0`, which forces full damage unconditionally, and its `RenderOutputResult`
      is discarded — so it is inert. Measured at 1280x720/120, `vaapi-dmabuf`, release,
      Radeon 880M:

      | Scenario | Frames with zero damage | Bitrate |
      |---|---|---|
      | Empty desktop | 241/241 — 100% | 2.00 Mbps |
      | Idle terminal (blinking cursor) | 228/232 — 98.3% | 1.94 Mbps |

      Cost vs. the no-session floor: CPU 13.0% of a core → 0.0%, GPU busy 13% → 8%.
      Encode is 3.4 ms of the 8.33 ms frame budget, i.e. 41% of the period recoverable.

      **Why the codec does not already do this:** `avg_frame_bytes` is 2095 — constant to
      the byte, in every scenario, and 2 Mbps ÷ 8 ÷ 120 = 2083. CBR (invariant #7) makes
      the encoder *stuff* frames that have nothing to encode, so a static screen costs
      exactly what a moving one does. Damage-gating is the only mechanism that makes
      static content cheap while CBR holds.

      **Bounds on the win:** ~98–100% of bandwidth, CPU and encode GPU on idle or
      near-idle content — most of a desktop's life, and the battery win lands on *both*
      ends, since the phone stops decoding 120 fps of an unchanging image. Exactly **zero**
      on full-motion content, and **zero for latency**: dragging a window damages every
      frame, so the interactive path is the one case this never touches. This is a
      power-and-bandwidth feature, not a latency one.

      **Queued behind the jitter work**, and deliberately: skipping produces irregular
      frame intervals, which is exactly what the playout buffer has to absorb — the buffer
      currently under suspicion. Settle the `W.playoutMs` sweep first.

      Needs, beyond flipping `age` to 1 and honouring the result: a heartbeat floor (~1 fps)
      so a long gap is not read as a stall, an IDR on resume and for any newly-joined viewer
      (`ForceKeyframe` exists), and a check that the capture target genuinely never swaps
      buffers — if it ever does, `age: 1` becomes a lie and the result is stale-content
      artefacts. `age: 1` alone is worthless: measured capture 0.15 ms → 0.016 ms, encode
      unchanged. The win is entirely in not encoding.
- [ ] **Additional hardware encoder tiers** — NVENC, AMF, QSV, VideoToolbox. Only VA-API and
      x264 exist; the tier ladder is already the right shape to extend.
- [ ] **H.265 / AV1 tiers**, gated on the client advertising hardware decode (invariant #4).
      Needs a codec-capability wire type.
- [ ] **Built-in TURN in the relay**, so it is self-contained. Until then, deploy coturn
      alongside.
- [ ] **Per-client `Output` sizing at connect** — M5, and the headline differentiator. Blocked
      on M4 finishing.
- [ ] **Native Android client** — hardware H.264 decode beats the browser on latency. M5/M6.
- [ ] **Persistent relay registry** so registrations survive a relay restart.
- [ ] **Multi-relay support** — register with several; clients fall back; nearest-relay
      selection.
- [ ] **Relay dashboard** — connected servers, active rooms, auth-failure rate.
- [ ] **Connection QR code** so a phone can join without typing an ID.
- [x] **Approval UI notification** ✓ `2026-10-02`: a sheet on the already-connected device (`js/seat.js`). Needs the confirmation gate first.

---

## DEFERRED

Parked deliberately. Each needs either hardware we do not have or a scope that is not active.

- [ ] **Kinetic scroll.** *Two-finger scroll landed (`input_scroll.js`), unconfirmed on a phone — see `plan/TODO.md`.*
- [x] **Pinch-zoom** ✓ found done in audit `2026-09-29` (`input/pinch.rs`); unconfirmed on a phone.
- [ ] **Touch drag-and-drop** — complex, and currently pre-empted by the long-press grab.
- [ ] **Discrete high-resolution wheel** — normalised pixel deltas are fine for most apps.
- [ ] **Child-process supervisor for the compositor** — the real fix for native crashes
      (panics are already contained, segfaults are not). The typed channel boundary was
      shaped for this move.
- [ ] **Trickle ICE** — client→server candidates already trickle (`IceCandidate`); the server's answer still waits for gathering, bounded by `ice::GATHER_WAIT`. Fast enough on
      LAN/known-STUN.
- [x] **NAT-type detection** ✓ found done in audit `2026-09-29` (`server/src/nat.rs`, symmetric-NAT probe at startup). — ICE handles it anyway; this is cosmetic prediction.
- [ ] **Session encryption beyond DTLS-SRTP.** DTLS-SRTP already covers the media plane, and
      the relay never sees media — only signalling. The remaining gap is the control plane,
      which is plain HTTP today; the auth gate and TLS in NECESSARY close it, so nothing more
      exotic is needed unless the threat model changes.
- [ ] **Parallel GStreamer capture/encode backend** — the documented exit if DMA-BUF modifier
      bugs ever get expensive. The capture trait is already backend-neutral for this.
- [x] **Relay↔server heartbeat / reconnect** — exponential backoff, reset on successful
      register. ✓ `2026-06-10`

---

## HYGIENE

Debt that taxes every future session.

- [ ] **`files.css` hard-codes colour** (`#888`, `#eee`, `#fff`, `rgb(20 20 22 / .94)` in the
      viewer) against the Sixteen Slots Rule in `DESIGN.md`. Move to base16 tokens.
- [x] **Split the server's website module.** ✓ `2026-09-29` — `routes`/`offer`/`pump`/`sse`/`http`/`watchdog`. ~21 KB doing routing *and* WebRTC setup *and*
      SSE *and* the frame pump — four jobs in one file, against the project's top coding
      rule. The SSE-events work above is a natural moment to cut it.
- [x] **Fix the `cargo fmt` baseline.** ✓ `2026-09-29` — `1ff767f`; keep `cargo fmt --all --check` green. The repo is not format-clean, so `cargo fmt` cannot
      be run repo-wide without burying real diffs in churn. Needs one deliberate
      formatting-only commit, then it stays clean.
- [x] **There are no tests.** ✓ `2026-09-29` — claim was stale (107 existed). Added the tier ladder + `RelayMsg` wire names; `map_point` needs a Smithay `Space`, gestures are JS — not covered. Eight suites, zero assertions — the build is the only gate.
      The input coordinate mapping, the gesture state machine, the tier-selection ladder and
      the protocol round-trip are all pure logic and cheap to cover.
- [x] **Relay mode has no timing endpoint** ✓ `2026-09-29` — `TimingRequest` already existed; fixed relay `queue_ms` being always 0., so the latency breakdown is direct-mode only.
- [x] **Audit docs against code.** ✓ `2026-09-29` — see the Decision Log entry; stale items below corrected. A status section once described an entire SSE event
      chain — module, type and all — that was never written. Claims about wiring need a grep
      before they are trusted.

---

## Done — condensed history

Newest first. Full rationale for each is in `WADO_PLAN.md`'s Decision Log.

- `2026-09-11` — **Latency work.** Input was never flushed to Wayland until the next render
  tick (quantising every input to the frame period — the jitter). Render timer accumulated
  lateness and could latch into a busy loop that starved input. No receiver playout control,
  so the browser held 100 ms+ invisibly to RTT. Drag motion was sent uncoalesced on every
  pointer move, saturating a reliable channel. Fixed: post-dispatch flush, timer clamp,
  playout hint, one shared positional coalescer, two input channels (motion unordered), frame
  queue 4→2, real elapsed-time RTP stamps. Added per-stage timing telemetry and a Debug
  breakdown toggle.
- `2026-09-11` — Dev shell: nixpkgs updated, ffmpeg held at 8.x, `wasm-bindgen` pin moved in
  lockstep with `dx`, libva pointed at the shell's own driver.
- `2026-06-10` — **M4 iteration 1**: relay crate, single Remote ID model, relay-restart
  resilience.
- `2026-06-08` — Encoder: zero-latency lock, on-demand keyframes, watchdog.
- `2026-06-07` — AGPL-3.0-only adopted across all crates.
- `2026-06-06` — Workspace split; client reimplemented in Dioxus (web/WASM); streaming
  pipeline hardened; central error type; live log panel.
- `2026-06-05` — Transport pivoted to WebRTC; M0 threshold met and verified.
