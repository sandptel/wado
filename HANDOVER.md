# Handover — wado at v0.0.4 (`2026-10-30`)

Everything built in the October run is merged on `main` and tagged **v0.0.4**. This file is the
starting point for picking the remaining work up **one item at a time**. Read `CLAUDE.md` first
(invariants, hazards), then this file, then the item's linked spec, plan or log.

## 1. Where things are

| | |
|---|---|
| Code | `main`, tag `v0.0.4`. All agent branches are merged. Only `experimental/desktop-app` (the F8 Slint app, 3 commits) is kept apart. |
| What changed | `CHANGELOG.md` → v0.0.4 (highlights at the top) |
| Why things are the way they are | `WADO_PLAN.md` Decision Log (gitignored, local); the entries dated `2026-10-29`/`30` cover this run |
| Specs and plans | `docs/superpowers/specs/2026-10-29…`/`2026-10-30…` (trackpad, computer page, platform layer, multi-client, pocket console, virtual input); `docs/superpowers/plans/` |
| Research | `docs/superpowers/research/2026-10-30-touchpad-feel.md` |
| Live-testing issue log | `plan/live-issues.md` (gitignored, local); the important rows are copied into §3 below |
| Run list | `plan/TODO.md` (gitignored, local) |

### Build, test, deploy

- **Daemon (release only, never debug):** `cargo build --release -p wado`, then swap the live
  daemon:
  1. SIGTERM its exact PID (argv `wado daemon`, never `wado tui`) and wait for it to exit.
  2. Relaunch detached with the environment copied from `/proc/<pid>/environ` (into
     `~/.cache/wado/daemon.env`: `rm -f` it first, then `cat` into it).
  3. Logs go to `~/.cache/wado/daemon.log`. For detail, run with
     `RUST_LOG=info,wado=debug,…`; the swap keeps the environment.
- **Web client:** pushing to `main` deploys GitHub Pages
  (`dx build … --release --debug-symbols false`). Locally a release build needs wasm-opt:
  `nix shell nixpkgs#binaryen -c dx build -p wado-client --release --platform web --base-path wado --debug-symbols false`.
- **APK:** `CARGO_TARGET_DIR=$PWD/target-android nix develop .#android -c scripts/android-build.sh --debug`
  produces `~/.cache/wado/wado-<ver>-debug.apk`. Use its own target dir: the Android shell's rustc
  differs from the default shell's.
- **Relay:** `scripts/relay-deploy.sh` (the binary is staged at `~/.cache/wado/wado-relay.bin`,
  glibc ≤ 2.34).
- **Tests:**
  - `cargo test --workspace --exclude wado-client` and `cargo test -p wado-client`
  - browser and e2e checks in `scripts/*.mjs`, each documented at its top:
    `passkey-ui-e2e`, `trap-check`, `console-check`, `layers-check`, `responsive-check`,
    `gamepad-check`, `pad-check`, `app-e2e`, `f3-e2e`, `files-e2e`, `lane-e2e`,
    `reconnect-time`, `multi-e2e`
  - Run them with the Bash sandbox off, and use `rtk proxy` to see unfiltered output.

## 2. Waiting on you (not code)

- [x] Relay deployed 2026-10-30 (v0.0.4, 12 caps, lane 64 KB/s). The previous binary is at `~/wado-relay.prev` on the VM.
- [ ] Kill leftover processes: `kill 637729 1031976` (old keep-awake holds that stop the laptop
      sleeping; the bug is fixed) and `kill 1101120 1101121` (a test server and Chrome).
- [ ] Optional: `wado allow "<device>" input` for the trackpad, and `join takeover` for older
      devices.
- [ ] A real-phone pass over v0.0.4. Nothing below was verified on hardware.
- [ ] Remove the merged agent worktrees and branches (`.claude/worktrees/agent-*`, 24 of them, all
      merged). Keep `experimental/desktop-app`.

## 3. Open work, ranked: take one at a time

Each item has what you'd see, the evidence, where to start, and a suggested approach.

### 0. Daemon hung when big clipboard data flooded the relay lane (2026-10-30 17:30Z) — fixed, not yet live
- **What it was** (from the log, no backtrace): the phone's Clipboard surface fetched every picture
  tile (`clip_get`), 31 pictures, 16 MB of `clip_data`. The relay then stopped taking the socket's
  writes: after `#67` at 17:30:26 no lane frame is ever reported again, though each would have
  queued over 5 s. The writer (`lane::pump`) sat in `sink.send`, stopped reading its channel
  (128), the channel filled, and the relay loop blocked in `send_relay(&out_tx, &Pong).await`, so
  it never got back to `timeout(RELAY_SILENCE, next())`. The futex is `block_on` parked on that
  await. At 17:45:26 the outbox's 16 MiB stop (`wants_more`) did the same thing on its own.
- **Fixed:**
  - `lane::pump` reads its channel always, including during a socket write. Bulk past the 16 MiB
    stop is dropped and logged (`lane: … dropped`). A write that takes nothing for 45 s ends the
    link (`WRITE_STALL`).
  - The relay loop reconnects when its writer ends, and `relay_watch::stuck` watches it from
    outside: `relay client: loop stuck for Ns — reconnecting`.
  - Clipboard copies over 1 MB stay off the lane, with a `clip_held` note in the device's words.
    No clipboard path is direct today, so these pictures never reach the phone. A direct carrier
    (the files channel?) is open work.
- **Regression test:** `lane::pump::tests::a_stuck_socket_never_stalls_the_reader`. It fails on
  the old pump with "the reader is stuck behind the lane".

### 0a. Small replies wait behind clipboard thumbnails on the lane (seen live 2026-10-31 21:21Z)
- **You see:** "the computer didn't answer" (shells list, host state) while the Clipboard screen is
  open over the relay.
- **Evidence:** 31 `clip_data` frames, about 6 MB of bulk, queued on the 64 KB/s lane. Waits grew to
  41 s (`slow: lane clip_data #272 took 40940 ms`), and `ask: shells_request / host_get still
  unanswered after 5 s` meanwhile. Hosts and shells share the bulk class, which drains first in,
  first out.
- **Fix:**
  - The daemon sends clipboard *thumbnails* downscaled (about 128 px WebP, a few KB) and the full
    picture only on demand.
  - The client fetches thumbnails only for visible tiles, a few at a time.
  - The lane gets a lowest "trickle" class for clipboard data, so host state, shells and file
    lists always go first.
- **Start at:** `crates/server/src/lane/outbox.rs` (classes), `crates/server/src/clip/serve.rs`
  (`ClipGet`), the client's `W.clipThumb` (`js/clipboard.js` / `cliphost.js`).

### 0b. Parallel use for every feature (decided 2026-10-30 — see Decision Log)
- **Goal:** all approved devices use Files, Clipboard, Shells, Claude, widgets, sessions (join and
  co-control) and their own gamepad at the same time. Only the host Touchpad and Keyboard are
  exclusive, each with an instant take over plus a toast and Take back. Nobody holds the seat to
  use a feature or to start a session.
- **Today:** many features ride the single relay *seat*, so a second device must "take over".
  F6's seatless `control` rooms (now live on the relay) give each device its own link.
- **Work list (check each, then move it off the seat):**
  - Files, clipboard, shells, Claude, push, widgets/host controls, config: on the control room.
  - Pad: per-feature holds (touchpad, keyboard) in the daemon, with "In use by …" on the door,
    tile and hub, plus take over and take back.
  - Gamepad: one virtual pad per device (`pad/game.rs`).
  - Desktop: join by default; Start my own; take over only for size.
  - Remove the seat-takeover prompts that stand in front of basic features.

### 1. No picture when the computer is on the phone's hotspot
- **You see:** "Connecting…" forever, then "Relayed — limited", no picture.
- **Evidence** (daemon log, 10:53Z): the laptop's `wlp97s0` is 10.83.90.214/24 via 10.83.90.22,
  and its IPv6 sits in the phone's cellular /64. The phone offers only cellular candidates (CLAT
  192.0.0.4 and its public IPv6). ICE checks for about 16 s, fails, and the phone re-offers in a
  loop.
- **Start at:** `crates/server/src/nat.rs`, `crates/server/src/viewers/pc.rs`, the client's ICE
  setup (`relay_link.js`, `session` offer).
- **Approach:**
  - Test whether the daemon's IPv6 host candidate reaches the phone's own IPv6 (Android may drop
    inbound tether → rmnet traffic).
  - Test a candidate on the 10.83.90.x link.
  - Otherwise, TURN on the relay as the honest fallback (`ice.rs` already has the hook).

### 2. Live-testing issues (`plan/live-issues.md`)
| # | You see | Lead |
|---|---|---|
| L10 | No picture for 30 s after a network blip; the daemon answered the rejoin twice, but the client never streamed | the `slow:` lane lines and the client ask clock (`fa815de`) now show where it stalls; reproduce with a forced relay drop |
| S1 | "The computer didn't answer" while something else is slow | `relay_client.rs` main loop awaits every handler inline; move slow handlers to tasks; the `slow:` WARN shows which |
| L11 | Speed test "no answer" | `linktest::send` queues ~530 KB sealed; check it against the lane |
| L4 | Picture ~130–150 ms behind after an app launch plus autorate dropping | browser jitter buffer; `reconnect-time.mjs` style measurement |
| L5/L6 | Trackpad arrivals late or starved on cellular; the pad link rebuilt on every rotation | see item 5; keep the pad link across rotation |
| L1 | A gesture dropped: `bad input event … null` (NaN coordinate) | the warning now prints the raw event; guard the client's input math against zero rects |
| L12 | Lutris tapped, nothing appears, no message | Lutris itself crashes (pygobject, no X). The compositor now logs "exited without showing a window"; surface it (item 3) |
| L7/L8/L9 | Log lines dropped when the lane is busy; a seek on a player that can't seek; SCTP log noise | small |

Timings on the computer side are fine: session stop 9–94 ms, start 87 ms to active, Chrome launch
~1.2–1.5 s. So most "no response" is missing feedback (item 3) plus the stalls above.

### 3. Feedback on every action, and console responsiveness
- **Built but not wired:**
  - `js/pending.js`, `src/pending.rs`, `assets/pending.css` (pressed / working / taking long /
    outcome)
  - additive acks on the wire (`ack` on SessionStop/Launch/Window/Detach, HostGet/HostDo,
    ConfigSet, ClipboardSet; `SessionDetached`, `ConfigSaved`, `ClipboardSent`)
  - `WindowList.launches`: starting / shown / exited
- **Next:** wrap the JS chokepoints so every caller gets feedback, sending `ack` on each request:
  - `W.launch`, `W.windowAction` (`control.js`)
  - `W.stopSession`/`relayStop`, `W.leaveSession`, `W.sessionAct`
  - `W.hostDo` (and `host_get`)
  - `W.configSet`, `clipboardFromPhone`, `claudeAnswer`
  - files rename/trash/mkdir around `F.req`
  - pad actions through `ui/computer/input/open.rs::run_action`

  Then add `scripts/feedback-check.mjs`. Also: an unrelated `session_error` must not tear the
  session UI down (in `relay_link.js` `fire`, skip `startFailed` when `pending.wire` says `"fail"`).
- **Console perf baseline** (`scripts/console-perf.mjs`, 390×844, 4× CPU): press shows after
  111–304 ms, input-to-paint up to 256 ms (608 ms at 6× in landscape full screen), 4 long tasks
  per 4 s while idle. Look for re-render storms and per-frame layout writes in `stage_fit.js`.
- Add the new stage states to `layers-check`.

### 4. Virtual input leftovers
- **T19, on real hardware:** a phone, a game, `evtest`, a live `hyprctl keyword`, "no stuck
  button after killing the browser", latency.
- `gamepad-check` has no daemon phase: copy `passkey-ui-e2e`'s bwrap and relay setup, send
  `game_on`/`btn`/`axes`.
- Sessionless Keys & mouse (T18): needs `KeyInjector::press` and a relative pointer device.
- Small:
  - the editor can't rebind keys in Keys mode
  - the home widget doesn't tint the last door
  - the native trackpad's "More" opens the web trackpad, not the Touchpad page
- Hyprland feel isn't re-applied on `configreloaded`, and a Lua config (`hyprctl eval`) is
  unsupported (`platform/linux/hypr_device.rs`, `pad/feel.rs`).
- Click bar and thumb marking are stored, but caps `pad-click` / `pad-palm` aren't built
  (`protocol/pad.rs`, `platform/linux/touchpad.rs`, new `js/pad_click.js`, `js/pad_palm.js`).
- `responsive-check` at 390×844: 36 of 240 states fail on the gamepad screens (`.gpc`) and the
  Computer page's `.vidoorname`.

### 4b. Touchpad variants (asked 2026-10-30, not designed yet)
- **The user asked for:** several touchpad *types*, like the gamepad's layouts, with the same
  editor and an animated, "surreal" feel (/motion-design). Candidates:
  - **Plain**: today's surface.
  - **Laptop**: a click bar with Left / Right (also covers research P0-5, for when tap-to-click is
    off).
  - **Edge scroll**: a vertical strip on the right and a horizontal one at the bottom, scrolling
    1:1.
  - **Split**: a pointer area plus a scroll wheel area.
  - **Precision**: a smaller, slower surface with a pointer-speed boost button.
  - **Media**: a surface plus a volume/seek ring.
  - Each has look options (glow trails, contact rings, a grid or texture), size and position.
- **Build it in the native app (Compose, P3 Virtual input)** first, reusing the gamepad's layout
  model (`js/gpad/*` documents it) so one model covers gamepad and touchpad. The web version
  follows from the same model.

### 5. Touchpad feel (from the research doc)
- **Frame timing per input batch:** stamp each frame with the event's time, one frame per batch.
  Then retune the pacer's 40 ms ceiling (it sits at max and still starves). Files: `pad_touch.js`,
  `pad_pointer.js`, `server/pad/pacer.rs`, `delay.rs`.
- Android Wi-Fi low-latency lock while the pad is open (`PadLink.kt`).
- Mirror Hyprland's gestures on the phone ("Lift: …", 1:1 track): `pad/host_gestures.rs`,
  `pad_hints.js`.
- Haptics on thresholds: `native_feel.js`, `Haptics.kt`.
- Measure first: research §8, `libinput record`, a constant-speed scripted stroke.

### 6. Native Android app — Jetpack Compose (framework decided 2026-10-30, plan not written)
- **Decision** (Decision Log `2026-10-30`): the app goes fully native and the WebView is removed.
  The web client stays for browsers and iOS.
- The protocol moves into the Rust core (`crates/mobile-core`): relay link, E2E, pairing,
  codecs. It is shareable with the desktop app.
- The plan must cover what the WebView got wrong: IME insets, the default video poster, a
  floating connecting state, hints over controls, losing the route on resume, ICE with the
  computer on the phone's hotspot.
- Also asked for: one native home, a distro/OS logo per computer (Simple Icons, CC0), natural
  quick controls per device, "On this phone" in the native home.
- Write `docs/superpowers/plans/2026-10-30-native-android.md` first.

### 7. Smaller
- F6: measure encode headroom, the lead's glass-to-glass with 1–3 viewers, and the cost of the
  keyframe on join.
- F5: `maximized` in `WindowInfo` (so Maximise/Restore can morph), a people card, a computed
  contrast pass, a screen-reader pass.
- F9b (subsumed by item 6): media3 MediaSession, release minify, run the CI job `android.yml`.
- A release signing key (`docs/release-signing.md`), when you want pinned updates.
