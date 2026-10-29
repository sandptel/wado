# Trackpad mode, plan 1 — touchpad + keyboard into the host — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A phone opens a **Trackpad** screen and becomes the computer's multi-touch touchpad and keyboard: the computer's own desktop, with no picture and no session, over a direct link or, failing that, the relay.

**Architecture:** The daemon gets a new `pad` module that owns two uinput devices: a multi-touch touchpad, so libinput and Hyprland interpret every gesture, and a keyboard. Text goes through `wtype` on the desktop's display. The phone sends **full-state contact frames** (every finger, every time, newest-wins), so a lost or late frame can never leave a finger stuck. It sends them on its own peer connection (an unreliable touch channel and a reliable keys channel). Until that connection is up, or if it never comes up, the same messages go through the sealed relay link, where a host-side playout queue evens out the bursts. The client screen is a JS-owned overlay, like the file manager, because it handles touch at 60–120 Hz.

**Tech Stack:** Rust (tokio, webrtc-rs 0.17, `libc` raw ioctls), `wado_protocol` serde types, browser JS bridge (`crates/client/src/js/`), Dioxus 0.7 for the landing button, node check scripts in `scripts/`.

**Spec:** `docs/superpowers/specs/2026-10-29-trackpad-mode-design.md`. Plans 2 (the actions library, settings and quick bar) and 3 (phone gestures) follow this one. Nothing here builds them.

## Global Constraints

- Touchpad and keyboard require the **new `input` grant**, **off by default** for every device, new and legacy (`Grants::LEGACY.input == false`). Checked on **every** message, never only at open.
- Refusal copy (exact): `this device may not use the trackpad — on the computer: wado allow <device> input`.
- No uinput copy (exact): `The trackpad needs /dev/uinput on the computer. On NixOS: hardware.uinput.enable = true, and add yourself to the uinput group.`
- Touch traffic never shares an SCTP association with files or video (invariant 1). The relay fallback is the logged exception (Decision Log `2026-10-29`).
- Colour only from base16 tokens in `assets/base.css`. Targets ≥ 44 px. Text inputs exactly 16 px. Motion via `transform`/`opacity` with `--spring`/`--pop`. Honour `data-motion` and `prefers-reduced-motion` (`DESIGN.md`).
- One job per file (CLAUDE.md). Server code holds no Smithay type.
- `cargo fmt --all --check` clean before every commit. Commits are short, with **no AI attribution** of any kind.
- The daemon under test is always a **release** build, swapped in after each build (`plan/memory/shared/environment.md`).
- Latency is reported as measured numbers, never asserted.

## Review Focus

1. **The phone browser is killed mid-drag or mid-Super-hold.** Expect no stuck finger and no stuck key on the computer within 400 ms. Pinned in Task 5 (`silence_lifts_all`) and Task 6 (teardown releases held modifiers).
2. **The phone rotates while a finger is down.** Expect the old device's contacts to be lifted before it is recreated at the new size, so no ghost contact. Pinned in Task 5 (`resize_lifts_then_recreates`).
3. **A relay stall delivers a burst of frames.** Expect the cursor to catch up without replaying the backlog, and a tap inside the burst still to click. Pinned in Task 5 (`burst_collapses_but_keeps_taps`).
4. **The grant is revoked from `wado tui` while the trackpad is open.** Expect the next frame to be refused and the devices torn down. Pinned in Task 6 (`revoked_grant_refuses_next_message`).
5. **A wado session starts while the trackpad is open.** Expect text still to go to the desktop, not to wado's own compositor. The compositor overwrites `WAYLAND_DISPLAY`, so text must use `crate::clip::tool::command`. Pinned in Task 4 (`text_targets_the_desktop_display`).

---

### Task 1: The `input` grant

**Files:**
- Modify: `crates/server/src/gate/grants.rs` (struct, `TOKENS`, `NONE`/`ALL`/`LEGACY`, `set`, `union`, `flags`, tests)
- Modify: `crates/server/src/tui/app.rs:53` (`GRANTS`) and `flip()` (~line 412)
- Modify: `crates/server/src/tui/view/devices.rs` (`COLS`, the column `match`)
- Modify: `crates/server/src/cli/devices.rs` (~line 70, the flag row)
- Modify: `crates/config/src/schema/security.rs` (the `new_device` doc comment lists tokens; the default stays `"host"`)

**Interfaces:**
- Produces: `Grants.input: bool`; the token `"input"`, accepted by `Grants::parse`, `wado allow`, pairing codes and the TUI.

- [ ] **Step 1: Write the failing tests** in `grants.rs`'s `mod tests`:

```rust
#[test]
fn input_is_its_own_token_and_off_by_default() {
    let g = Grants::parse("host input").unwrap();
    assert!(g.input && g.host && !g.shells);
    assert_eq!(Grants::parse(&g.tokens()).unwrap(), g);
    assert!(!Grants::LEGACY.input);
    assert!(!Grants::parse("host").unwrap().input);
    assert!(Grants::ALL.input);
}
```

- [ ] **Step 2: Run it.** `cargo test -p wado input_is_its_own_token` → FAIL (no field `input`).
- [ ] **Step 3: Add the field everywhere the other booleans appear.** Keep the token order `files-ro, files-rw, shells, settings, host, input, clipboard`. In the TUI, the new column is `("input", "inp")`, before `clipboard`, so `flip` and the column `match` shift by one. Add `input` to the CLI flag row and the `security.rs` comment.
- [ ] **Step 4: Run.** `cargo test -p wado grants` → PASS (all four tests). `cargo build -p wado` → green.
- [ ] **Step 5: Commit.** `git commit -m "gate: input grant, off by default"`

---

### Task 2: The wire

**Files:**
- Create: `crates/protocol/src/pad.rs`
- Modify: `crates/protocol/src/lib.rs` (`pub mod pad;`)
- Modify: `crates/protocol/src/relay.rs`: four variants after `FilesNote`

**Interfaces:**
- Produces, in `wado_protocol::pad`:

```rust
pub const TOUCH_CHANNEL: &str = "pad-touch"; // unordered, maxRetransmits 0
pub const KEYS_CHANNEL: &str = "pad-keys";   // ordered, reliable
pub type Contact = (u8, u16, u16);           // (id, x, y); x/y normalised 0..=65535 over the pad

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mod { Ctrl, Alt, Shift, Super }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")] // not "t": Frame has a field `t`
pub enum PadMsg {
    /// The pad's size in CSS px. Sent first, and again on every resize.
    Size { w: u16, h: u16 },
    /// Every finger down right now. Empty = all lifted. `t` = client ms, wrapping.
    Frame { seq: u32, t: u32, c: Vec<Contact> },
    /// A named key (`esc`, `tab`, `enter`, `backspace`, `delete`, `left`, `right`, `up`,
    /// `down`, `home`, `end`, `pageup`, `pagedown`, `space`, `f1`…`f12`) tapped with `mods`.
    Key { key: String, mods: Vec<Mod> },
    /// One character tapped as a key with `mods` (a latched modifier + the next character).
    Char { ch: char, mods: Vec<Mod> },
    /// Modifiers held down until the next `Hold` (a locked modifier). `[]` releases all.
    Hold { mods: Vec<Mod> },
    /// Committed text: typed as text, any Unicode.
    Text { text: String },
    Ping { t: u32 },
    /// Daemon → phone only.
    Pong { t: u32 },
    /// Daemon → phone only: a message was refused, or a device could not be made.
    Err { message: String },
}

/// Is `seq` newer than `last`, across u32 wrap.
pub fn newer(seq: u32, last: u32) -> bool
```

- `RelayMsg` gains `PadOffer { sdp: String }`, `PadAnswer { #[serde(default)] sdp: String, #[serde(default)] err: String }`, `PadIn { msg: PadMsg }` (client → daemon) and `PadOut { msg: PadMsg }` (daemon → client). These are post-handshake variants: the relay forwards them unread and **needs no relay change**.

- [ ] **Step 1: Write the failing tests** in `pad.rs`:

```rust
#[test]
fn frame_round_trips_compactly() {
    let m = PadMsg::Frame { seq: 7, t: 1000, c: vec![(0, 100, 65535)] };
    let s = serde_json::to_string(&m).unwrap();
    assert_eq!(s, r#"{"k":"frame","seq":7,"t":1000,"c":[[0,100,65535]]}"#);
    assert_eq!(serde_json::from_str::<PadMsg>(&s).unwrap(), m);
}
#[test]
fn newer_survives_wrap() {
    assert!(newer(1, 0));
    assert!(!newer(0, 0));
    assert!(newer(2, u32::MAX - 1));
    assert!(!newer(u32::MAX - 1, 2));
}
#[test]
fn pad_in_rides_the_relay_enum() {
    let r = crate::relay::RelayMsg::PadIn { msg: PadMsg::Hold { mods: vec![Mod::Super] } };
    let s = serde_json::to_string(&r).unwrap();
    assert!(s.starts_with(r#"{"type":"pad_in""#));
}
```

- [ ] **Step 2: Run.** `cargo test -p wado-protocol pad` → FAIL (module missing).
- [ ] **Step 3: Implement** `pad.rs` and the four `RelayMsg` variants. `newer` is `seq.wrapping_sub(last).wrapping_sub(1) < u32::MAX / 2`.
- [ ] **Step 4: Run.** `cargo test -p wado-protocol` → PASS, including the existing relay-mirror test. `cargo build --workspace` → green.
- [ ] **Step 5: Commit.** `git commit -m "protocol: pad wire"`

---

### Task 3: The touchpad device

**Files:**
- Create: `crates/server/src/pad/mod.rs` (module doc + `pub mod` lines only for now)
- Create: `crates/server/src/pad/uinput.rs`
- Create: `crates/server/src/pad/touchpad.rs`
- Modify: `crates/server/src/lib.rs` (`pub mod pad;`)

**Interfaces:**
- Produces:
  - `uinput::Device::open(name: &str, setup: impl FnOnce(&Device) -> io::Result<()>) -> io::Result<Device>`
  - `Device::{evbit(u16), keybit(u16), absbit(u16), propbit(u16), abs(code: u16, max: i32, res: i32), emit(&[(u16, u16, i32)])}`. `emit` appends `SYN_REPORT`, and `Drop` destroys the device.
  - `touchpad::slot_events(prev: &[Contact], next: &[Contact], w: i32, h: i32) -> Vec<(u16, u16, i32)>`: pure, no device. `w`/`h` are the device's axis maxima, and a position is `x as i64 * w as i64 / 65535`.
  - `touchpad::Touchpad::new(w_px: u16, h_px: u16) -> io::Result<Touchpad>`, `.frame(&mut self, c: &[Contact]) -> io::Result<()>`, `.size(&self) -> (u16, u16)`.

Constants to use (`uinput.rs`):

```rust
const UI_DEV_SETUP: u64 = 0x405c_5503;  // _IOW('U', 3, struct uinput_setup) — 92 bytes
const UI_ABS_SETUP: u64 = 0x401c_5504;  // _IOW('U', 4, struct uinput_abs_setup) — 28 bytes
const UI_SET_EVBIT: u64 = 0x4004_5564;
const UI_SET_KEYBIT: u64 = 0x4004_5565;
const UI_SET_ABSBIT: u64 = 0x4004_5567;
const UI_SET_PROPBIT: u64 = 0x4004_556e;
const UI_DEV_CREATE: u64 = 0x5501;
const UI_DEV_DESTROY: u64 = 0x5502;
```

Touchpad (`touchpad.rs`):
- The name is `"wado trackpad"`, with bus `BUS_VIRTUAL` (0x06).
- Props: `INPUT_PROP_POINTER` (0x00) and `INPUT_PROP_BUTTONPAD` (0x02).
- Keys: `BTN_LEFT` 0x110, `BTN_TOUCH` 0x14a, `BTN_TOOL_FINGER` 0x145, `DOUBLETAP` 0x14d, `TRIPLETAP` 0x14e, `QUADTAP` 0x14f, `QUINTTAP` 0x148.
- Abs: `ABS_X`/`ABS_Y` (0x00/0x01), `ABS_MT_SLOT` 0x2f (max 4), `ABS_MT_POSITION_X/Y` 0x35/0x36, `ABS_MT_TRACKING_ID` 0x39 (max 65535).
- Range `0..=w_px*10` × `0..=h_px*10`, with resolution `CSS_PX_PER_MM * 10` where `const CSS_PX_PER_MM: i32 = 6;` (≈ a phone's CSS px; `ponytail:` comment naming it as the knob if pointer speed feels off).
- `slot_events` maps contact id → slot by first-free (ids are client-assigned, 0..=9; at most 5 slots and extra fingers are ignored). It emits, in order:
  1. per slot that lifted, `MT_SLOT s, MT_TRACKING_ID -1`
  2. per slot that moved or appeared, `MT_SLOT s` (+ `TRACKING_ID` on appear) + `POSITION_X/Y`
  3. `BTN_TOUCH`, and the `BTN_TOOL_*` for the new finger count (old one 0, new one 1)
  4. `ABS_X/ABS_Y` of the lowest slot

- [ ] **Step 1: Write the failing tests** in `touchpad.rs`:

```rust
#[test]
fn one_finger_down_moves_then_lifts() {
    let down = slot_events(&[], &[(0, 32768, 32768)], 3900, 8000);
    assert!(down.contains(&(EV_ABS, ABS_MT_TRACKING_ID, 0)));
    assert!(down.contains(&(EV_KEY, BTN_TOUCH, 1)));
    assert!(down.contains(&(EV_KEY, BTN_TOOL_FINGER, 1)));
    assert!(down.contains(&(EV_ABS, ABS_MT_POSITION_X, 1950)));
    let up = slot_events(&[(0, 32768, 32768)], &[], 3900, 8000);
    assert!(up.contains(&(EV_ABS, ABS_MT_TRACKING_ID, -1)));
    assert!(up.contains(&(EV_KEY, BTN_TOUCH, 0)));
    assert!(up.contains(&(EV_KEY, BTN_TOOL_FINGER, 0)));
}
#[test]
fn second_finger_switches_tool_to_doubletap() {
    let e = slot_events(&[(0, 0, 0)], &[(0, 0, 0), (1, 10, 10)], 100, 100);
    assert!(e.contains(&(EV_KEY, BTN_TOOL_FINGER, 0)));
    assert!(e.contains(&(EV_KEY, BTN_TOOL_DOUBLETAP, 1)));
}
#[test]
fn a_sixth_finger_is_ignored() {
    let five: Vec<Contact> = (0..5).map(|i| (i, 0, 0)).collect();
    let mut six = five.clone(); six.push((5, 0, 0));
    assert!(slot_events(&five, &six, 100, 100).is_empty());
}
#[test]
#[ignore = "needs /dev/uinput"]
fn device_is_created_and_destroyed() {
    let mut t = Touchpad::new(390, 800).unwrap();
    t.frame(&[(0, 100, 100)]).unwrap();
    t.frame(&[]).unwrap();
}
```

- [ ] **Step 2: Run.** `cargo test -p wado touchpad` → FAIL (module missing).
- [ ] **Step 3: Implement** `uinput.rs`: raw `libc::ioctl` on `/dev/uinput` opened `O_WRONLY | O_NONBLOCK`, with events written as 24-byte `input_event` (zero timestamp). Follow `crates/compositor/src/input/gamepad.rs`, but use the modern `UI_DEV_SETUP`/`UI_ABS_SETUP` path, because the legacy one cannot carry a resolution. Add a `ponytail:` comment that names the duplication with `gamepad.rs` and the upgrade (one shared micro-crate). Implement `touchpad.rs` as specified.
- [ ] **Step 4: Run.** `cargo test -p wado touchpad` → 3 PASS, 1 ignored. Then `cargo test -p wado touchpad -- --ignored` (sandbox **disabled**) → PASS. While a `sleep` is added to that test, `libinput list-devices | grep -A3 "wado trackpad"` shows `Capabilities: pointer gesture`.
- [ ] **Step 5: Commit.** `git commit -m "pad: uinput touchpad"`

---

### Task 4: Keyboard device and text

**Files:**
- Create: `crates/server/src/pad/keys.rs`
- Create: `crates/server/src/pad/text.rs`

**Interfaces:**
- Produces:
  - `keys::key_code(name: &str) -> Option<u16>`, `keys::char_code(ch: char) -> Option<(u16, bool /*shift*/)>`, `keys::mod_code(m: Mod) -> u16`
  - `keys::chord_events(mods: &[Mod], code: u16) -> Vec<(u16, i32)>`: pure, mods down in the given order, key down, key up, mods up in reverse.
  - `keys::Keyboard::new() -> io::Result<Keyboard>`, `.tap(&mut self, mods: &[Mod], code: u16)`, `.hold(&mut self, mods: &[Mod])` (diffs against what it holds), `.release_all(&mut self)`. `Drop` calls `release_all`.
  - `text::args(text: &str) -> Vec<&str>` → `["--", text]`; `text::type_text(text: &str) -> Result<(), String>` (async).

Values:
- `mod_code`: Ctrl 29, Alt 56, Shift 42, Super 125.
- `key_code`: esc 1, tab 15, enter 28, backspace 14, delete 111, left 105, right 106, up 103, down 108, home 102, end 107, pageup 104, pagedown 109, space 57, f1–f10 59–68, f11 87, f12 88.
- `char_code` covers the **US layout**: a–z, A–Z, 0–9, space, `\n` (enter), `\t` (tab), ``-=[]\;',./` `` and their shifted forms ``_+{}|:"<>?~!@#$%^&*()``. Add a `ponytail:` comment: US table; non-US hosts get the wrong punctuation; the upgrade is xkbcommon lookup of the host keymap.
- The keyboard device is named `"wado keyboard"` with `EV_KEY` bits 1..=255.

- [ ] **Step 1: Write the failing tests:**

```rust
// keys.rs
#[test]
fn us_table() {
    assert_eq!(char_code('a'), Some((30, false)));
    assert_eq!(char_code('A'), Some((30, true)));
    assert_eq!(char_code('1'), Some((2, false)));
    assert_eq!(char_code('!'), Some((2, true)));
    assert_eq!(char_code(' '), Some((57, false)));
    assert_eq!(char_code('é'), None);
    assert_eq!(key_code("esc"), Some(1));
    assert_eq!(key_code("nope"), None);
}
#[test]
fn chord_presses_mods_around_the_key() {
    assert_eq!(chord_events(&[Mod::Super], 2), vec![(125, 1), (2, 1), (2, 0), (125, 0)]);
    assert_eq!(chord_events(&[Mod::Ctrl, Mod::Shift], 30),
        vec![(29, 1), (42, 1), (30, 1), (30, 0), (42, 0), (29, 0)]);
}
// text.rs
#[test]
fn text_is_one_argument_after_double_dash() {
    assert_eq!(args("-rf / ; ls"), vec!["--", "-rf / ; ls"]);
}
#[test]
fn text_targets_the_desktop_display() {
    // Review Focus 5: the compositor overwrites WAYLAND_DISPLAY; text must use the captured one.
    crate::clip::env::capture();
    let src = include_str!("text.rs");
    assert!(src.contains("clip::tool::command(\"wtype\")"));
}
```

- [ ] **Step 2: Run.** `cargo test -p wado pad::keys pad::text` → FAIL.
- [ ] **Step 3: Implement.** `type_text` builds `crate::clip::tool::command("wtype")?`, adds `args(text)`, and runs it with a 5 s deadline. If `wtype` is missing, the error is exactly: `Typing text needs wtype on the computer — install it (keys and shortcuts still work).`
- [ ] **Step 4: Run.** The same command → PASS.
- [ ] **Step 5: Commit.** `git commit -m "pad: keyboard and text"`

---

### Task 5: Pad state and relay playout

**Files:**
- Create: `crates/server/src/pad/state.rs`
- Create: `crates/server/src/pad/playout.rs`

**Interfaces:**
- Consumes: Task 2 `PadMsg`, `newer`; Task 3 `Touchpad`; Task 4 `Keyboard`, `type_text`.
- Produces:
  - `playout::Playout::new() -> Playout`; `.push(&mut self, now_ms: u64, t: u32, c: Vec<Contact>)`; `.due(&mut self, now_ms: u64) -> Vec<Vec<Contact>>`; `.next_wake(&self) -> Option<u64>`.
  - `state::Pad::new(sink: Box<dyn Sink>) -> Pad`, where `trait Sink: Send { fn touch(&mut self, size: (u16, u16), c: &[Contact]) -> Result<(), String>; fn tap(&mut self, mods: &[Mod], code: u16) -> Result<(), String>; fn hold(&mut self, mods: &[Mod]) -> Result<(), String>; fn release_all(&mut self); }`. The real sink owns `Touchpad` + `Keyboard` and recreates the touchpad on a new size. The test sink records calls.
  - `Pad::handle(&mut self, now_ms: u64, msg: PadMsg, relayed: bool) -> Option<PadMsg>` returns a reply (`Pong`, `Err`). Text is **not** typed here; `handle` returns it via `Pad::take_text(&mut self) -> Option<String>` for the async caller.
  - `Pad::tick(&mut self, now_ms: u64)`: releases due playout frames and enforces the silence lift.
  - `pub const SILENCE_MS: u64 = 400;`, `pub const PLAYOUT_DELAY_MS: u64 = 24;`, `pub const WINDOW_MS: u64 = 2000;`

Rules (`state.rs`):
- `Frame` with `!newer(seq, last_seq)` is dropped.
- Direct frames go to the sink at once. Relayed frames go through `Playout`.
- If any contact has been down and no frame arrived for `SILENCE_MS`, `touch(size, &[])`.
- `Size` that differs from the current size by more than 2 px first calls `touch(old, &[])`, then switches size.
- `Char` with no `char_code` replies `Err { message: "“é” has no key on this keyboard layout — type it as text instead" }` (the char quoted).

Playout algorithm (`playout.rs`), relay path only:

```text
offset = min over the last WINDOW_MS of (arrival_ms − t)      // sliding minimum
due(frame) = frame.t + offset + PLAYOUT_DELAY_MS
due(now): pop every frame with due ≤ now, in order. Of consecutive popped frames with the SAME
set of contact ids, keep only the last; never drop a frame whose id set differs from the next
one (that frame is a touch-down or a lift).
```

- [ ] **Step 1: Write the failing tests:**

```rust
// playout.rs
#[test]
fn steady_frames_come_out_spaced() {
    let mut p = Playout::new();
    p.push(1000, 0, vec![(0, 1, 1)]);
    p.push(1016, 16, vec![(0, 2, 2)]);
    assert_eq!(p.due(1023).len(), 0);
    assert_eq!(p.due(1024), vec![vec![(0, 1, 1)]]);
    assert_eq!(p.due(1040), vec![vec![(0, 2, 2)]]);
}
#[test]
fn burst_collapses_but_keeps_taps() {
    let mut p = Playout::new();
    p.push(1000, 0, vec![(0, 1, 1)]);           // baseline offset 1000
    // a 300 ms stall, then everything at once
    for (i, f) in [vec![(0, 2, 2)], vec![(0, 3, 3)], vec![], vec![(1, 9, 9)], vec![]].into_iter().enumerate() {
        p.push(1316, 16 * (i as u32 + 1), f);
    }
    let out = p.due(2000);
    // the moves collapse into the last one before the lift; the tap (down, lift) survives
    assert_eq!(out, vec![vec![(0, 3, 3)], vec![], vec![(1, 9, 9)], vec![]]);
}
// state.rs (with a recording sink)
#[test] fn stale_seq_is_dropped() { /* frame seq 5 then seq 4 → sink saw one touch */ }
#[test] fn silence_lifts_all() { /* frame with a contact at 0 ms; tick(399) → no lift; tick(401) → touch(&[]) */ }
#[test] fn resize_lifts_then_recreates() { /* Size 390×800, contact down, Size 800×390 → touch(old, []) before any touch at the new size */ }
#[test] fn ping_is_answered() { /* handle(Ping{t:5}) == Some(Pong{t:5}) */ }
#[test] fn char_without_key_says_so() { /* Char{ch:'é'} → Some(Err{..}) whose message contains "é" */ }
```

- [ ] **Step 2: Run.** `cargo test -p wado pad::` → FAIL.
- [ ] **Step 3: Implement** both files as specified.
- [ ] **Step 4: Run.** `cargo test -p wado pad::` → all PASS.
- [ ] **Step 5: Commit.** `git commit -m "pad: state and relay playout"`

---

### Task 6: Daemon transports and gating

**Files:**
- Create: `crates/server/src/pad/channel.rs`
- Create: `crates/server/src/pad/gate.rs`
- Modify: `crates/server/src/pad/mod.rs` (doc lists every file; `pub struct Live` below)
- Modify: `crates/server/src/relay_client.rs`: context field `pad: pad::Live` (next to `files_pc`, ~line 81/486); handlers for `PadOffer` and `PadIn` next to `FilesOffer` (~line 1586); `pad.close()` at the three `close_files(&ctx.files_pc)` sites (~1329, ~1613, ~2008)

**Interfaces:**
- Consumes: Task 1 `Grants.input`; Task 2 wire; Task 5 `Pad`.
- Produces:
  - `pad::gate::refused(gate: &Gate, device_key: &str, approved: bool) -> Option<String>`: `None` when allowed. Its copy comes from Global Constraints (`<device>` = the device name).
  - `pad::Live` (cheaply `Clone`): `Live::default()`, `.open(&self, api: &API, offer: &str, who: files::channel::Who, gate: Gate, out: OutTx) -> Result<String /*answer sdp*/, String>`, `.relayed(&self, msg: PadMsg, who_key: &str, gate: &Gate, out: &OutTx)`, `.close(&self)`.
  - `OutTx` is whatever `send_relay` takes in `relay_client.rs`. Replies on the relay path go out as `RelayMsg::PadOut`.

Behaviour:
- `Live` holds one `Pad` behind a `Mutex`, shared by both transports, so a phone can start relayed and switch to direct mid-drag without a ghost contact. It also runs a 10 ms tokio interval while open, calling `Pad::tick`.
- The **first** message that passes the gate creates the devices. If `/dev/uinput` cannot be opened, the reply is `Err` with the no-uinput copy, and the pad stays closed.
- `channel.rs` mirrors `files/channel.rs`: a peer connection with `TOUCH_CHANNEL` (frames) and `KEYS_CHANNEL` (everything else, replies included). Each message is re-checked with `gate::refused` **before** `Pad::handle`; a refusal replies `Err` and calls `close()`.
- `close()` calls `release_all`, drops both devices and closes the peer connection.

- [ ] **Step 1: Write the failing tests** in `gate.rs`:

```rust
#[test]
fn needs_input_not_host() {
    let gate = Gate::for_test(&[("k1", "phone", "host"), ("k2", "tab", "input")]);
    assert!(refused(&gate, "k1", true).unwrap().contains("wado allow phone input"));
    assert!(refused(&gate, "k2", true).is_none());
    assert!(refused(&gate, "k2", false).unwrap().contains("not been approved"));
}
#[test]
fn revoked_grant_refuses_next_message() {
    let gate = Gate::for_test(&[("k2", "tab", "input")]);
    assert!(refused(&gate, "k2", true).is_none());
    gate.grant("tab", "-input").unwrap();
    assert!(refused(&gate, "k2", true).is_some());
}
```

> If `Gate` has no test constructor, add `#[cfg(test)] pub fn for_test(lines: &[(&str, &str, &str)]) -> Gate` in `gate/mod.rs`. It writes a temp `trusted_clients` and reads it the way `Gate::default` does. Check how `gate.grant` spells a removal (`-input` vs `input off`) and match it.

- [ ] **Step 2: Run.** `cargo test -p wado pad::gate` → FAIL.
- [ ] **Step 3: Implement** `gate.rs`, `channel.rs`, `Live` and the `relay_client.rs` wiring. Log one line on open (`info!(device, transport, "pad: open")`) and one on close.
- [ ] **Step 4: Run.** `cargo test -p wado` → all PASS. `cargo build --release -p wado` → green. `cargo fmt --all --check` → clean.
- [ ] **Step 5: Commit.** `git commit -m "pad: transports and per-message gate"`

---

### Task 7: Client link

**Files:**
- Create: `crates/client/src/js/pad_link.js`
- Modify: `crates/client/src/bridge.rs` (`include_str!("js/pad_link.js")` after `files_link.js`)
- Create: `scripts/pad-check.mjs` (grows in Tasks 8–9)

**Interfaces:**
- Produces `W.pad`:
  - `W.pad.start()`: begins relayed at once (messages go as `{type:"pad_in", msg}`), then negotiates direct (`pad_offer`/`pad_answer`, same envelope path as `files_link.js`)
  - `W.pad.stop()`
  - `W.pad.send(msg, lossy)`: `lossy` frames use the touch channel when direct, else the relay; everything else uses the keys channel when direct, else the relay
  - `W.pad.state`: `"off" | "relayed" | "direct"`
  - `W.pad.onState(fn)`, `W.pad.onMsg(fn)` (for `pad_out` and direct replies)
  - `W.pad.rtt`: ms, from a `Ping` every 2 s with `Pong` matched by `t`
- On switching relayed → direct, it re-sends `Size` and the current frame on the new path, so the daemon's state is complete.
- If the direct offer is refused (`err` set), it stays relayed only if the err is about the path. A grant refusal stops the pad and surfaces the err through `onMsg({k:"err", message})`.

- [ ] **Step 1: Write the failing check** in `scripts/pad-check.mjs`. Follow `scripts/touch-check.mjs`: load `pad_link.js` into a stub `W` with fake `relaySendMsg`, `RTCPeerConnection` and clock. Assert:
  - after `start()`, `send({k:"frame",…}, true)` reaches `relaySendMsg` as `{type:"pad_in", msg:{k:"frame",…}}`
  - after the fake data channels open, the same call goes to the touch channel and `state === "direct"`, and the switch re-sent `size` first
  - a `pad_answer` with `err:"…wado allow phone input"` makes `state === "off"` and fires `onMsg` with that message
- [ ] **Step 2: Run.** `node scripts/pad-check.mjs` → FAIL.
- [ ] **Step 3: Implement** `pad_link.js`. Touch channel: `createDataChannel("pad-touch", {ordered:false, maxRetransmits:0})`. Keys: `{ordered:true}`. The STUN servers and the 3 s gathering bound are as in `files_link.js`. The direct attempt gives up after 8 s and stays relayed. While relayed, it retries direct after a network change (`W.relayOn("__up")`).
- [ ] **Step 4: Run.** `node scripts/pad-check.mjs` → PASS.
- [ ] **Step 5: Commit.** `git commit -m "client: pad link, relay first then direct"`

---

### Task 8: Client touch surface

**Files:**
- Create: `crates/client/src/js/pad_touch.js`
- Modify: `crates/client/src/bridge.rs` (after `pad_link.js`)
- Modify: `scripts/pad-check.mjs`

**Interfaces:**
- Consumes: `W.pad.send`, `W.pad.state`.
- Produces: `W.padTouch.attach(el: HTMLElement)`, `W.padTouch.detach()`, `W.padTouch.frame()` (the current contacts as the wire array, for the link's re-send).

Behaviour:
- Pointer events with `pointerType === "touch"`, plus mouse and pen, are treated as one finger. Each new pointer gets the lowest free id 0..=9. x/y are `Math.round(frac * 65535)` over `el.getBoundingClientRect()`, clamped. `touch-action: none` is on `el`.
- **Direct:** one frame per event-loop turn that changed anything (flush in a microtask).
- **Relayed:** a fixed 16 ms tick, sending only if changed.
- **Both:** a keepalive re-send every 100 ms while any contact is down. `Size` on attach and on `ResizeObserver` change. `seq` increments per frame; `t` is `performance.now()|0`.
- `pointercancel` and `lostpointercapture` lift that contact. `visibilitychange` → hidden sends an empty frame at once.

- [ ] **Step 1: Write the failing checks.** With a fake 1000×1000 element and a fake clock:
  - direct: down at (500,500), move to (510,500), up → three frames, the last with `c: []`, with ids and coordinates `[[0,32768,32768]]`, then `[[0,33423,32768]]`
  - relayed: ten moves within 16 ms → one frame
  - holding still 250 ms → at least 2 keepalive frames, all with the same `c`
  - two fingers → ids 0 and 1; lifting finger 0 and pressing again reuses id 0
  - `pointercancel` → an empty frame
- [ ] **Step 2: Run.** `node scripts/pad-check.mjs` → FAIL on the new checks.
- [ ] **Step 3: Implement** `pad_touch.js`.
- [ ] **Step 4: Run.** `node scripts/pad-check.mjs` → PASS.
- [ ] **Step 5: Commit.** `git commit -m "client: pad touch frames"`

---

### Task 9: Client keyboard logic

**Files:**
- Create: `crates/client/src/js/pad_keys.js`
- Modify: `crates/client/src/bridge.rs`, `scripts/pad-check.mjs`

**Interfaces:**
- Produces:
  - `W.padKeys.diffText(prev: string, next: string) -> {back: number, ins: string}`: pure
  - `W.padKeys.attach(input: HTMLTextAreaElement, row: HTMLElement)`, `W.padKeys.detach()`
  - `W.padKeys.mods`: `{ctrl, alt, shift, super}`, each `"off"|"latched"|"locked"`
  - `W.padKeys.onMods(fn)`: so the UI paints the row

Behaviour:
- **diffText:** the common prefix by code points (not UTF-16 units). `back` = remaining prev code points, `ins` = remaining next.
- **Text line:** keep `sent`. Act on `input` events with `!e.isComposing`, and on `compositionend`. For the diff: send `back` × `{k:"key", key:"backspace", mods:[]}`, then `{k:"text", text: ins}` if non-empty. A `\n` inside `ins` becomes a `key enter`. When the field passes 200 characters, clear it and reset `sent` (`ponytail:` the window keeps autocorrect working without growing forever).
- **Latched mods:** if any mod is latched and the diff is exactly `{back:0, ins: one code point}`, send `{k:"char", ch, mods}`, clear the latches and remove that char from the field (it was a shortcut, not text).
- **Modifier row:**
  - tap: off → latched, or latched → off
  - double-tap within 300 ms: locked, which sends `{k:"hold", mods:[locked…]}`
  - tap on a locked mod: off, which sends `hold` without it
  - the special keys on the row (Esc, Tab, and the arrows in a second rank) send `{k:"key", key, mods: latched+locked}`, then clear the latches
- Backspace and Enter from the phone keyboard arrive through the diff. `keydown` with `key === "Enter"` on a single-line commit is also caught, because some IMEs never insert `\n`.

- [ ] **Step 1: Write the failing checks:**

```js
eq(diffText("hello", "hello w"), { back: 0, ins: " w" });
eq(diffText("teh", "the "), { back: 2, ins: "he " });   // autocorrect replaced the word
eq(diffText("a😀", "a"), { back: 1, ins: "" });         // one code point, not two units
// latched Super, then the phone keyboard commits "1"
//   → sent [{k:"char", ch:"1", mods:["super"]}], the field is empty, the latch is off
// double-tap Ctrl → sent {k:"hold", mods:["ctrl"]}; tap Ctrl again → {k:"hold", mods:[]}
// dictation commits "Hello there." in one compositionend
//   → exactly one {k:"text", text:"Hello there."}
```

- [ ] **Step 2: Run.** `node scripts/pad-check.mjs` → FAIL.
- [ ] **Step 3: Implement** `pad_keys.js`.
- [ ] **Step 4: Run.** `node scripts/pad-check.mjs` → PASS.
- [ ] **Step 5: Commit.** `git commit -m "client: pad keyboard, latched and locked modifiers"`

---

### Task 10: The Trackpad screen

**Files:**
- Create: `crates/client/src/js/pad_ui.js` (owns `#wado-pad` outright, like `files_ui.js` owns `#wado-files`)
- Create: `crates/client/assets/pad.css`
- Modify: `crates/client/src/bridge.rs` (after `pad_keys.js`); the stylesheet list wherever `files.css` is linked (`grep -rn files.css crates/client`)
- Modify: `crates/client/src/ui/landing/mod.rs`: an `iconbtn` in `landingfoot` next to Files, relay mode only. `aria-label`/`title` is `"Use as trackpad — no video"`, the icon is the closest existing one in `ui/widgets/icon.rs` (add a `trackpad` glyph there if none fits), and `onclick` → `bridge::call("window.__wado.padOpen();")`

**Interfaces:**
- Consumes: `W.pad`, `W.padTouch`, `W.padKeys`, `W.wake`.
- Produces: `W.padOpen()`, `W.padClose()`.

Screen (layout A, `DESIGN.md`):
- **Full-screen overlay** at z-index 60, above the landing (40) and below the link badge (65) and toasts (70). It sits on `--s0` with safe-area padding.
- **Header row:**
  - a close button: `iconbtn`, 44 px, `aria-label="Close trackpad"`
  - a **status pill** in the `.linkpill` language. It reads `Direct · 12 ms` (good tone) / `Relayed · 85 ms` (warn tone) / `Connecting…` (dim), with the number from `W.pad.rtt`. Tapping it unfolds one plain line: *Relayed: "No direct path to the computer — touches go through wado's relay, so expect some delay."*
- **The pad:** `flex: 1`, `border-radius: var(--r4)`, `background: var(--s1)`, `border: 1px solid var(--line)`, with a one-line dim hint centred until the first touch ("Two fingers to scroll · your computer's gestures work here"). Each finger shows a 26 px accent ring, drawn with `transform` only.
- **Keyboard button:** full width, `.btn`, 48 px, label "Keyboard". It toggles the keyboard area:
  - the modifier row: Super, Ctrl, Alt, Shift, Esc, Tab, then ← ↑ ↓ →, 44 px each, horizontally scrollable on narrow phones
  - latched = an accent ring; locked = an accent fill (`.tile.on` language)
  - a 16 px `textarea` that is focused when opened, with `autocapitalize="sentences" autocomplete="off" enterkeyhint="enter"`
  - the pad keeps `flex: 1` above it, using `visualViewport` to stay above the phone keyboard
- **Errors** from `W.pad.onMsg` show as a `.card.alertcard` over the pad, with the daemon's message verbatim.
- **Behaviour:**
  - `padOpen` → `W.pad.start()`, `W.wake.acquire()`, and pushes a history entry so the Android Back gesture closes the pad instead of leaving the app
  - `padClose` → sends an empty frame and `hold []`, `W.pad.stop()`, `W.wake.release()`

- [ ] **Step 1: Build and check by eye.** `dx build` (client) → green. Open the client in Chrome at **390×844** and **1280×800**, with the `default-dark` scheme and one light scheme, against a release daemon. Screenshot: the pad closed, the keyboard open, a latched modifier, the relayed pill. Check the base16-only colours, 44 px targets, no horizontal scroll, and focus rings on every control.
- [ ] **Step 2: Fix what one look shows**, then re-shoot once.
- [ ] **Step 3: Run every check.** `node scripts/pad-check.mjs` → PASS. `cargo test --workspace` → PASS. `cargo fmt --all --check` → clean.
- [ ] **Step 4: Commit and push** (push to main deploys the client). `git commit -m "client: trackpad screen"`

---

### Task 11: On the real machine

**Files:**
- Modify: `TODO.md` (local): a **Trackpad mode** item group with the phone checklist below
- Modify: `plan/memory/` (lane `ui/` or `shared/`): what was learned, measured numbers

- [ ] **Step 1: Release-swap the daemon** (`cargo build --release -p wado`, restart per `plan/memory/shared/environment.md`), then `wado allow <phone> input`.
- [ ] **Step 2: Host checks, run here with the sandbox disabled.** Open the trackpad from headless Chrome (or the phone) and confirm that `libinput list-devices` shows `wado trackpad` (pointer gesture) and `wado keyboard`. Close the tab: within 1 s both are gone (`libinput list-devices` again).
- [ ] **Step 3: Record the input round trip** from the pill (or the `pad: open` log + `Ping`/`Pong`). Do this on LAN direct, then relayed (Direct blocked, e.g. the phone on mobile data). Write p50/p90 into `plan/memory/latency/measurements.md`.
- [ ] **Step 4: Hand the user the phone checklist (spec § Success):**
  - 20 taps = 20 clicks
  - two-finger scroll never clicks
  - a three-finger Hyprland swipe always switches workspace
  - a resting thumb at the edge doesn't move the pointer
  - dictating "Hello, café." lands exactly
  - Super then 1 switches workspace
  - a locked Super + drag moves a window
  - killing the browser mid-drag leaves nothing stuck
  - relayed, the cursor moves without catch-up bursts
- [ ] **Step 5: Commit** any fixes; update `TODO.md` and memory.

---

## Self-review notes

- **Spec coverage:** touchpad (T3, T5), keyboard + dictation + sticky/locked modifiers (T4, T9), `input` grant (T1, T6), works with no session (T6 rides the relay link, not a session), works everywhere: relay-first then direct, with playout (T5, T7), teardown with no stuck keys (T5, T6, T11), layout A + keyboard area + honest pill (T10), wake lock (T10), measured latency (T7 rtt, T11), the bad-detection checks (T11).
- **Spec items deliberately left to plans 2 and 3:** the actions library, the Actions settings page, the quick bar (B's rail), phone gestures. Layout A's Settings button lands with plan 2, because in this plan it would open a page with nothing on it (the "never show a control that does nothing" rule).
