# macOS host (no compositor) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `wado daemon` builds and runs on macOS without the compositor, serves Files, Clipboard,
Input, Media, Volume, Brightness and the extras through `wado-platform` traits, announces itself as
macOS, and both clients show a macOS variant.

**Architecture:** Linux code is untouched except for `cfg(target_os = "linux")` gates and
target-specific dependencies. On macOS a stand-in crate (`wado-session-macos`) supplies the
compositor boundary the server names (commands, input sender, handles, frame type) and answers
every session command with "coming to macOS". The server names that boundary only through
`crate::compositor` (one re-export file, picked by `cfg`). A macOS provider crate
(`wado-host-macos`) implements the platform traits with objc2 bindings.

**Tech Stack:** Rust 2024, objc2 family (`objc2`, `objc2-foundation`, `objc2-app-kit`,
`objc2-core-graphics`, `objc2-core-audio`), CoreGraphics CGEvent, CoreAudio, NSPasteboard,
NSWorkspace, private DisplayServices via `dlopen`, AppleScript via `osascript`; Flutter 3.47.0;
Dioxus 0.7.

**Spec:** `plan/superpowers/specs/2026-11-09-macos-host-design.md`

## Global Constraints

- Linux behaviour unchanged; Linux code only gains `cfg` gates. Linux is proven by CI only (it cannot be built here).
- Work on branch `macos`; never push `main`.
- Commit messages: one short line, `scope: what changed`, never mention Claude/AI.
- Opening is the probe (invariant 6): a macOS capability is `On` only if opening it works here; otherwise `Off` with a reason naming the System Settings pane.
- A private API (DisplayServices, symbolic hotkeys) is resolved at runtime and fails to "absent", never crashes.
- No new dependency where one in the tree does it; objc2 crates are the one new family.
- `cargo fmt --all --check` green.
- Login item (`wado install`) exists but is never enabled during testing.

## Review Focus

1. **Accessibility not granted** — CGEvents are silently dropped. Expect: `keys`/`touch`/`text` Off with "Allow wado in System Settings › Privacy & Security › Accessibility", re-probed so granting it turns them on without a restart. (Task 5 test: probe with a fake trust check returns Off + that reason.)
2. **Modifier left held** after a chord or a dropped connection — later clicks become ⌘-clicks. Expect: `release_all` on drop and on `lift_all`; every posted event carries explicit flags. (Task 5 test: tap(⌘, C) then a click event has empty flags.)
3. **Symlink or `..` escaping the shared folder** on macOS. Expect: same refusals as Linux. (Task 7 tests: symlink in middle component, `..`, absolute name, rename onto existing name.)
4. **Old client meets the Mac daemon / new client meets an old Linux daemon.** Expect: old client ignores new fields; new client without `platform` assumes Linux. (Task 3 test: deserialize ConfigState without the fields.)
5. **Clipboard echo loop** — wado writes the pasteboard, sees the change, sends it back. Expect: own writes not re-announced. (Task 6 test with a fake pasteboard: set then poll yields no change event.)

---

### Task 1: CI that proves Linux still builds

**Files:**
- Create: `.github/workflows/host.yml`

- [ ] **Step 1:** Read `flake.nix` default devShell for the Linux system libraries; write a workflow with two jobs on push to `macos`: `linux` (ubuntu-latest, apt the same libs: wayland, gbm, drm, va, ffmpeg dev, x264, xkbcommon, udev, pkg-config, clang) running `cargo check --workspace --all-targets` + `cargo test -p wado -p wado-platform -p wado-protocol -p wado-config`; `macos` (macos-latest) running `cargo test -p wado -p wado-host-macos -p wado-session-macos -p wado-platform -p wado-protocol`.
- [ ] **Step 2:** Push the branch; expected: linux job green on the unmodified tree (baseline before any change). If the linux job is red at baseline, fix the workflow, not the code.
- [ ] **Step 3:** Commit `ci: linux + macos host checks`.

### Task 2: The daemon builds on macOS

**Files:**
- Create: `crates/macos/session/{Cargo.toml,src/lib.rs}` (package `wado-session-macos`)
- Create: `crates/server/src/compositor.rs`
- Modify: `Cargo.toml` (members), `crates/server/Cargo.toml` (target tables), every `wado_compositor::` in `crates/server/src` → `crate::compositor::`
- Modify: `crates/server/src/platform/mod.rs`, `main.rs`, `lib.rs`, plus `cfg` gates where Linux-only libc / tools are named (`files/*`, `platform/linux/*`, `audio/`, `pad/game.rs`)

**Interfaces:**
- Produces (`wado-session-macos`, same names/fields the server uses from `wado-compositor`): `CompositorCommand` (all variants, protocol payload types), `CommandSender { fn send(&self, CompositorCommand) -> Result<(), Closed> }`, `InputSender { send, send_from, gone, lead }`, `InputItem`, `Origin`, `PRIMARY`, `InputEvent` (re-export), `FrameMsg`, `CompositorHandles` (same fields), `CompositorError`, `clipboard::Clip` (= `wado_platform::clip::Clip`), `hit::{HitWindow, MenuSpot}` (same fields), `input::gamepad::live_count() -> usize` (0), `backend::Session` implementing `wado_platform::video::VideoBackend` with `Model::Mirror`, `Thread::Any`.
- `crate::compositor` re-exports `wado_compositor::*` on Linux and `wado_session_macos::*` on macOS.

- [ ] **Step 1: failing test** in `crates/macos/session/src/lib.rs`:
  `start_is_refused_as_coming`: build handles, send `Start`, reply is `Err(s)` with `s.contains("coming to macOS")`; `status_is_none`; `running_apps_is_empty`; `hit_window_is_none`.
- [ ] **Step 2:** `cargo test -p wado-session-macos` → fails (crate empty).
- [ ] **Step 3:** Implement: one thread owning an `std::sync::mpsc::Receiver<CompositorCommand>` answering replies as above, ignoring the rest; watches created with defaults and kept alive by the runner.
- [ ] **Step 4:** Make `wado-compositor`, `ffmpeg-the-third`, `x264`-users Linux-only target deps of `wado`; gate `audio`, `platform::linux`, Linux syscalls; `platform::os_default()` picks `MacProvider` (empty stub until Task 4). Iterate `cargo build -p wado` to zero errors, `cargo test -p wado` green on macOS.
- [ ] **Step 5:** Run `./target/release/wado daemon` against the public relay; expected log: `host capabilities probed` and relay registered (`/health` `servers` ≥ 1).
- [ ] **Step 6:** Push; CI linux + macos green. Commit `server: builds on macOS without the compositor`.

### Task 3: The Mac announces itself (platform + capability states)

**Files:**
- Modify: `crates/protocol/src/caps.rs` (`CapOff.later: bool`, serde default), `crates/platform/src/cap.rs` (`Absent::later(why)`), `crates/protocol` ConfigState (or the message `config/link.rs` builds): `platform: String` (`"linux"`/`"macos"`, serde default `"linux"` on read), `host_caps: CapSet`
- Modify: `crates/server/src/config/link.rs`, `crates/server/src/platform/mod.rs` (`fn daemon_features() -> Vec<(name, Option<CapOff>)>` for `session`, `sound`, `windows`, `apps_in_session`, `notifications`, `menus`)

**Interfaces:**
- Produces: wire `config.platform`, `config.host_caps = { on: [names], off: [{cap, why, later}] }`. Names = `Capability::name()` plus the daemon features above.

- [ ] **Step 1: failing tests:** `capoff_without_later_reads_false`; `config_without_platform_reads_linux`; `macos_marks_session_as_later` (on macOS cfg: `session` in `off` with `later == true`); `linux_has_session_on` (Linux cfg).
- [ ] **Step 2–4:** fail → implement → pass.
- [ ] **Step 5:** Commit `protocol: host platform and capability states`.

### Task 4: macOS provider + extras (sysinfo, awake, power, pty, launcher, screenshot)

**Files:**
- Create: `crates/macos/host/{Cargo.toml,src/lib.rs,src/sysinfo.rs,src/power.rs,src/launch.rs,src/shot.rs,src/tcc.rs}` (package `wado-host-macos`)
- Modify: `crates/server/src/platform/mod.rs` (register `wado_host_macos::MacProvider`), PTY: reuse the Linux `portable-pty` provider if it compiles on macOS (move it to a shared module), else a macOS copy.

**Interfaces:**
- Produces: `MacProvider: wado_platform::Provider`; `tcc::accessibility() -> bool` (AXIsProcessTrusted), `tcc::screen_recording() -> bool` (CGPreflightScreenCaptureAccess); reasons as consts `NEEDS_ACCESSIBILITY`, `NEEDS_SCREEN_RECORDING`.
- SysInfo: `os = "macOS <ver>"`, `os_id = "macos"`, cpu = `machdep.cpu.brand_string`, mem via `host_statistics64`, hostname = ComputerName, desktop = "Aqua".
- PowerInhibit: `IOPMAssertionCreateWithName(PreventUserIdleSystemSleep)`; SystemPower: Lock = `SACLockScreenImmediate` (login.framework, dlopen) fallback ⌃⌘Q, Suspend = `pmset sleepnow`.
- Launcher: Terminal = `com.apple.Terminal`, Browser = default `https` handler, Files = Finder; via `open -b`. Screenshot: `screencapture -x [-i]` (needs Screen Recording; probe).

- [ ] **Step 1: tests:** `sysinfo_reads_macos` (os_id == "macos", mem_total_mb > 0, cores > 0); `power_inhibit_toggles_state` (set(true) → state true, assertion id held; set(false) → released); `pty_runs_echo` (spawn `/bin/echo hi`, output contains `hi`, exit 0).
- [ ] **Step 2–4:** fail → implement → pass (`cargo test -p wado-host-macos`).
- [ ] **Step 5:** daemon log lists `sys_info, power_inhibit, system_power, pty, launcher` on. Commit `host-macos: provider, sysinfo, power, pty, launcher, screenshot`.

### Task 5: Input — keys, text, trackpad, gamepad keys mode

**Files:**
- Create: `crates/macos/host/src/input/{mod.rs,keymap.rs,post.rs,keys.rs,text.rs,pad.rs,hotkeys.rs}`
- Test: in-module tests

**Interfaces:**
- `keymap::kvk(evdev: u16) -> Option<u16>` (full US map: letters, digits, F1–F20, arrows, nav, modifiers, media).
- `post` — the only file calling CGEvent: `trait Poster { fn key(&mut self, kvk: u16, down: bool, flags: u64); fn pointer(&mut self, ev: Pointer); fn scroll(&mut self, dx: f64, dy: f64, phase: Phase); fn text(&mut self, s: &str); }` with `CgPoster` (real) and a recording fake for tests.
- `pad::Interpreter { fn frame(&mut self, now: Instant, contacts: &[Contact], out: &mut dyn Poster) }` — pure state machine: 1 finger move → relative pointer (cursor position read once per gesture, clamped to the union of displays); tap < 180 ms & < 8 px → click; 2-finger tap → right click; tap-then-hold → drag (`LeftMouseDragged`); 2-finger move → pixel scroll with `ScrollPhase` began/changed/ended, then momentum (`MomentumPhase`) decaying from the last velocity, never both phases on one event; 3-finger swipe ≥ 25% pad → hotkey (up: Mission Control 32, down: App Exposé 33, left/right: Spaces 79/81 via `CGSGetSymbolicHotKeyValue`, fallback ⌃↑/⌃↓/⌃←/⌃→); pinch ratio > 1.25 / < 0.8 → ⌘+ / ⌘−.
- `MacTouch: TouchInjector` (Interpreter + CgPoster), `MacKeys: KeyInjector` (explicit flags on every event, `release_all` on drop), `MacText: TextInjector` (keycode 0 + `keyboardSetUnicodeString`, flags = [], ≤ 20 UTF-16 units per event), gamepad: `Gamepad` stays Off ("Not on macOS: no virtual controller without an Apple entitlement", later=false); the pad's "keys" mode reaches `KeyInjector` (verify in `crates/server/src/pad/game.rs` that keys mode is sessionless; if it is session-only, route it to `KeyInjector` on macOS).
- Probe: all Off with `NEEDS_ACCESSIBILITY` unless `AXIsProcessTrusted()`; re-probed every 5 s while Off.

- [ ] **Step 1: tests (fake Poster):** `kvk_maps_letters_and_modifiers` (KEY_A=30→0, KEY_LEFTMETA=125→55, KEY_ENTER=28→36); `tap_clicks`; `two_finger_tap_right_clicks`; `tap_hold_drags_with_dragged_events`; `two_finger_scroll_has_began_changed_ended_then_momentum`; `momentum_and_scroll_phase_never_on_one_event`; `three_finger_up_sends_mission_control`; `pinch_out_sends_cmd_plus`; `chord_then_click_has_clean_flags`; `drop_releases_held_mods`; `text_chunks_at_20_utf16_units`; `untrusted_probe_is_off_with_accessibility_reason`.
- [ ] **Step 2–4:** fail → implement → pass.
- [ ] **Step 5 (live, needs the user's Accessibility grant — ping):** with the daemon running, `wado` CLI pad smoke (`scripts/pad-smoke.mjs` if present, else a 20-line node client over the control room) moves the cursor and types `hello` into TextEdit. Commit `host-macos: keys, text and the trackpad`.

### Task 6: Clipboard (text, images, history)

**Files:**
- Create: `crates/macos/host/src/clip.rs`
- Modify: `crates/server/src/platform/mod.rs` if history needs the in-memory store (`clip/ram.rs`) registered as `ClipHistory` on macOS

**Interfaces:**
- `MacClipboard: Clipboard` — `get`: first of `public.utf8-plain-text` → `text/plain;charset=utf-8`, `public.png`/`public.tiff` → `image/png` (TIFF converted via `NSBitmapImageRep`); `secret` = `is_secret(types)`; `set`: `clearContents` + typed data + private marker type `org.wado.own`; `changes`: `changeCount` polled every 250 ms **only while a receiver exists** (watch `receiver_count`), own writes (marker present) not counted.
- History: wado's own store (reuse `clip/ram.rs` if it implements `ClipHistory`, else a 50-entry ring in this file, secrets never stored).

- [ ] **Step 1: tests:** `text_round_trips` (real pasteboard, a private `NSPasteboard(name:)` so the user's clipboard is untouched); `png_round_trips`; `concealed_is_secret`; `own_write_is_not_a_change`; `history_skips_secrets`.
- [ ] **Step 2–4:** fail → implement → pass. Commit `host-macos: clipboard and history`.

### Task 7: Files (scope, trash, thumbnails)

**Files:**
- Modify: `crates/server/src/files/scope.rs`, `files/list.rs`, `files/quick.rs`, `files/mod.rs` — split each Linux syscall into `files/os/{linux.rs,macos.rs}` behind one fn each: `open_beneath(dir: &File, rel: &Path, flags) -> io::Result<File>` (macOS: walk components with `openat(O_NOFOLLOW)`, refuse `..`/absolute, last component `O_NOFOLLOW_ANY`), `rename_noreplace(..)` (macOS `renameatx_np(RENAME_EXCL)`), `open_path_only` (macOS `O_RDONLY|O_DIRECTORY` or `O_EVTONLY`).
- Create: `crates/macos/host/src/files.rs` — `MacTrash: Trash` (`NSFileManager trashItemAtURL:resultingItemURL:`; `place()` = `~/.Trash`; `origin` from the recorded mapping; `forget` drops it), `MacThumbs: Thumbnailer` (`qlmanage -t -s 256 -o <tmp>` on the real path → PNG bytes; probe: `/usr/bin/qlmanage` exists).

- [ ] **Step 1: tests:** the existing `files` test suite passes on macOS unchanged (it is the spec), plus `symlink_component_refused`, `dotdot_refused`, `rename_onto_existing_refused`, `trash_then_listed_in_dot_trash`, `thumb_of_png_is_png`.
- [ ] **Step 2–4:** fail → implement → pass (`cargo test -p wado files`). Commit `files: macOS paths, trash and thumbnails`.

### Task 8: Media, volume, brightness

**Files:**
- Create: `crates/macos/host/src/{media.rs,audio.rs,brightness.rs}`

**Interfaces:**
- `MacMedia: MediaPlayers` — players from AppleScript for running `Music`/`Spotify` only (`application "X" is running` checked first so nothing is launched); `bus` = bundle id; title/artist/album/art (Spotify `artwork url`; Music art written to a temp PNG → `data:` URI) polled every 2 s while subscribed; `act`: PlayPause/Next/Previous via `NX_SYSDEFINED` media-key events (works for any app); Seek/volume ops via AppleScript for those two apps; a non-scriptable app's op returns `Err("controls only on macOS for <app>")`. Probe On always (media keys); Automation denial → that player's card says "Allow wado to control <app> in Privacy & Security › Automation".
- `MacAudio: AudioMixer` — `HostAudio` from CoreAudio: output devices (`kAudioHardwarePropertyDevices` with output streams), default output, its `VirtualMainVolume` + mute; `set_volume`/`set_mute` on device id; `set_default(name)`; `move_stream` → `Err` "Not on macOS: per-app audio". State pushed by CoreAudio property listeners.
- `MacBrightness: Brightness` — `dlopen("/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices")`, `DisplayServicesGetBrightness/SetBrightness(CGMainDisplayID(), f32)`; missing symbol or no built-in display → `step` only via brightness keys (`NX_KEYTYPE_BRIGHTNESS_UP/DOWN`), state `None`.

- [ ] **Step 1: tests:** `media_ops_map_to_keys` (fake poster: PlayPause → key 16 down/up); `applescript_parse_track` (parse `title|artist|album|state` fixture); `volume_round_trip` (real CoreAudio: read default output volume v, set v, read == v ± 0.01 — skip with message if no output device); `brightness_read_in_range` (skip if absent); `move_stream_is_not_on_macos`.
- [ ] **Step 2–4:** fail → implement → pass. Commit `host-macos: media, volume, brightness`.

### Task 9: Wado.app, signing, optional login item

**Files:**
- Create: `scripts/mac-app.sh`, `scripts/mac-cert.sh`, `packaging/macos/Info.plist`
- Modify: `crates/server/src/cli/*` — `wado install` / `wado uninstall` write/remove `~/Library/LaunchAgents/io.github.sandptel.wado.plist` (macOS only); **not run during testing**.

- [ ] **Step 1:** `mac-cert.sh` creates a self-signed code-signing identity `wado-dev` in the login keychain (openssl → p12 → `security import` + `add-trusted-cert -p codeSign`), idempotent.
- [ ] **Step 2:** `mac-app.sh` builds release, assembles `target/Wado.app` (`LSUIElement`, bundle id `io.github.sandptel.wado`, `NSAppleEventsUsageDescription`), `codesign -s wado-dev --options runtime`; `codesign -dv` shows `Authority=wado-dev`.
- [ ] **Step 3:** Commit `macos: Wado.app, dev signing, optional login item`.

### Task 10: Client variants (Android app + web)

**Files:**
- Modify (app): `app/lib/wado/relay_link.dart` or `Host` config reader → `Host.I.platform`, `Host.I.hostCaps`; new `app/lib/ui/computer/on_this_mac.dart` (the "On this Mac" card); gate surfaces in `computer_page.dart`, `sessions.dart`, `actions.dart`, `quick.dart`, `input_pages.dart`, `sound_devices.dart`, `pages.dart`; Apple logo in the device card.
- Modify (web): `crates/client/src/state.rs` (platform + caps), the matching Dioxus components and a `ui/on_this_mac.rs`.
- Shared helper semantics: `capState(name) -> on | later(why) | off(why)`; `later` → dimmed, badge **Coming to macOS**, tap shows why; `off` → hidden, listed in **On this Mac** with why; unknown platform → treat as Linux.

- [ ] **Step 1: tests:** Dart `test/host_caps_test.dart` (`later` parses, missing platform = linux, `off` hidden); Rust `crates/client` unit test for the same parse.
- [ ] **Step 2–4:** fail → implement → pass (`flutter test`, `cargo test -p wado-client` or `dx build`).
- [ ] **Step 5:** `scripts/app-build.sh --android --release` → APK in `~/Downloads`; `dx build --platform web --release` succeeds. Commit `app: macOS variant` / `web: macOS variant`.

### Task 11: End to end

- [ ] Wado.app running against the public relay; pairing link produced; ping the user to pair + grant Accessibility, Automation, Files prompts, Screen Recording.
- [ ] Synthetic checks after grants: keys/text into TextEdit, pointer moves, clipboard both ways, file list of `~/Downloads`, volume set/restore, brightness read, media key, screenshot file created.
- [ ] CI green on both jobs; `plan/TODO.md` + memory updated; phone checklist to the user.
