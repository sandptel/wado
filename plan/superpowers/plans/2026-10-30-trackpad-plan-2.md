# Trackpad plan 2 — landscape, rail, settings sheet, actions, phone gestures

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Every task
> that touches UI (Tasks 5–8) MUST first invoke the `motion-design` and `ui-ux-pro-max` skills
> (Skill tool) and apply them within `DESIGN.md`'s system. Steps use `- [ ]` checkboxes.

**Goal:** The trackpad becomes the couch tool the user actually holds. It runs in landscape by
default, with an optional left rail of keys or quick actions. It has a settings sheet inside the
pad, a library of actions that run on the user's Hyprland laptop, and a few phone-only gestures
the user can assign to those actions.

**Architecture:**
- The daemon gains an **actions** module: one file per group, plus detection of Hyprland and of
  the tools present.
- The phone asks over the existing pad link which actions this computer supports, and runs them by
  id. Actions need the `host` grant, checked on every message.
- Per-device trackpad choices live **typed** in `config.kdl` under `device "<key>" { trackpad { … } }`.
  The phone writes them through the existing ConfigSet path, and they are readable and editable in
  the file, `wado tui` and `wado devices`.
- Landscape without fullscreen: when the phone is upright, the trackpad draws itself rotated 90°
  and touch coordinates are rotated to match. A real `screen.orientation.lock` is used only when
  the user turns on fullscreen.

**Spec:** `docs/superpowers/specs/2026-10-29-trackpad-mode-design.md`, as amended by the Decision
Log `2026-10-30` entries ("Trackpad plan 2 scope"). Plan 1 (input) and the feel work (pacer +
touch feedback, branch merges of brief A/B) land first. **This plan starts from main after both
merge.**

## Global constraints

- **`DESIGN.md` is binding.** Base16 tokens only. Targets ≥ 44 px. Text inputs at 16 px. Motion
  via transform/opacity with `--pop`/`--spring`. Honour `prefers-reduced-motion` and
  `data-motion`. WCAG 2.2 AA on chrome.
- **Touch tracking is 1:1, never eased.** No feedback element may add latency to the send path
  (`js/pad_touch.js` decides what is sent and when).
- **Phone gestures never steal laptop gestures.** wado claims only motions a laptop touchpad never
  produces (edge swipe in, four-finger tap, two-finger long hold), and all are off by default.
  Two- and three-finger swipes and pinch are always forwarded.
- **Grants:** the touchpad and keyboard need `input`. Listing and running actions need `host`.
  Both are checked on every message. Copy: `this device may not run actions — on the computer:
  wado allow <device> host` (the device name quoted via `Gate::typed`).
- **Commands are arguments, never a shell string.** Tools run through the existing `host::cmd` /
  `clip::tool::command` pattern. Hyprland's instance signature is captured at daemon start (the
  `clip::env` pattern), with a fallback to the newest `$XDG_RUNTIME_DIR/hypr/*`.
- **An unavailable action is hidden, never a silent no-op.** A failed one replies in plain words.
- **Scope cut (user, 2026-10-30):** no key-combo buttons, no custom commands in v1. Record them in
  TODO as later.
- One job per file. `cargo fmt --all --check` and `node scripts/pad-check.mjs` stay green. Release
  daemon only. No AI attribution in commits.

## Review focus

1. **Rotated coordinates:** in rotated landscape, a finger moving right on the screen *as the user
   holds it* moves the laptop pointer right, and two-finger scroll scrolls the right way.
   (Task 5 test.)
2. **Hyprland restarted while the daemon runs** (new instance signature): window actions still
   work, or are hidden with a reason. (Task 3.)
3. **A three-finger Hyprland swipe never triggers a wado gesture**, and an edge swipe starting
   8–12 px in is not forwarded as a touch. (Task 8.)
4. **A device without `host` sees no actions** and gets the plain refusal, while its touchpad still
   works if it has `input`. (Task 4.)
5. **Settings survive a reload and another browser on the same device key** (stored in
   `config.kdl`, not only in localStorage). (Task 7.)

---

### Task 1: Per-device trackpad settings in config

**Files:**
- `crates/config/src/schema/device.rs`: add `trackpad: Option<DeviceTrackpad>`
- new `crates/config/src/schema/device_trackpad.rs`
- `crates/config/src/tier.rs`: a device's own `device.<its key>.trackpad.*` is settable by that
  device without the settings grant. Do this in `crates/server/src/config/link.rs::set`, which
  knows the requester's key.

**Shape (defaults):**

```rust
pub struct DeviceTrackpad {
    pub landscape: bool,           // true  — the user's main way of holding it
    pub fullscreen: bool,          // false — only when the user wants the real orientation lock
    pub flip: bool,                // false — which way round the rotated layout faces
    pub rail: String,              // "keys" | "actions" | "none"; default "keys"
    pub quick: Vec<String>,        // action ids on the rail/quick bar; default
                                   //   ["workspace-prev","workspace-next","fullscreen","play-pause"]
    pub gestures: BTreeMap<String, String>, // "edge-left"|"edge-right"|"four-tap"|"two-hold"
                                            //   → action id; default empty (all off)
    pub keyboard: bool,            // keyboard open when the pad opens; default false
}
```

**Tests:**
- Parse a `device "k" { trackpad { landscape #false; rail "actions"; quick "fullscreen" } }` block.
- An unknown `rail` value is an error.
- The requester may set its own `device.<key>.trackpad.rail` without `settings`, but may not set
  another device's.

---

### Task 2: Wire

**Files:** `crates/protocol/src/pad.rs` (additive variants only)

```rust
Actions,                                         // phone → daemon: what can run here?
ActionList { actions: Vec<ActionInfo> },         // daemon → phone
Run { id: String },                              // phone → daemon
Ran { id: String, ok: bool, #[serde(default)] message: String }, // daemon → phone
pub struct ActionInfo { id: String, name: String, group: String, icon: String }
```

**Tests:** round-trip JSON. The tag stays `k`.

---

### Task 3: Actions library (daemon)

**Files:** new `crates/server/src/actions/`, one job per file:
- `mod.rs`: the registry. It provides `list() -> Vec<ActionInfo>` (available only) and
  `run(id) -> Result<(), String>`.
- `detect.rs`: is Hyprland running (instance signature, `hyprctl version`)? Which tools exist?
  Cached, refreshed every 30 s or on a miss.
- `hypr.rs`: the instance signature (captured at start), the newest-dir fallback, and
  `dispatch(args)`.
- `window.rs`: `fullscreen`, `close-window`, `toggle-float`, `pin`, `focus-left`, `focus-right`.
  Mechanism: `hyprctl dispatch fullscreen` / `killactive` / `togglefloating` / `pin` /
  `movefocus l|r`.
- `workspace.rs`: `workspace-prev`, `workspace-next` (`workspace e-1/e+1`), `workspace-1…9`,
  `move-to-workspace-1…9` (`movetoworkspace N`).
- `media.rs`: `play-pause`, `next-track`, `prev-track`. Reuse `crate::host::media` (MPRIS). Do not
  shell out to playerctl.
- `sound.rs`: `volume-up`, `volume-down`, `mute`. Reuse `crate::host::audio` (wpctl).
- `system.rs`: `lock` (`loginctl lock-session`), `suspend` (`systemctl suspend`), `screenshot`
  (`grim` to `~/Pictures/wado-<time>.png`, with `slurp` for `screenshot-region` when present),
  `brightness-up/down` (`brightnessctl set 10%+/-`).
- `apps.rs`: `open-terminal` (`xdg-terminal-exec`, else `$TERMINAL`, else hidden), `open-browser`
  (`xdg-open https://`), `open-files` (`xdg-open ~`). Spawned detached, never awaited.

Each action carries `name` (plain words, e.g. "Next workspace"), `group` (Window / Workspace /
Media / Sound / System / Apps) and `icon` (a name from `ui/widgets/icon.rs`, or a new glyph added
there and in `pad_ui`'s inline set).

**Tests:**
- The registry lists only actions whose mechanism `detect` reports (use a fake detector).
- `workspace-next` builds exactly `["dispatch", "workspace", "e+1"]`.
- An unknown id gives `Err("unknown action …")`.
- Hyprland env resolution prefers the captured signature, else the newest dir.
- `#[ignore]` live tests (run unsandboxed on the user's machine): list on this laptop includes the
  Window group; `workspace-next` then `workspace-prev` returns to the same workspace
  (`hyprctl activeworkspace -j`).

---

### Task 4: Daemon wiring

**Files:** `crates/server/src/pad/live.rs` (or a new `pad/actions.rs` if `live.rs` would grow a
second job), `pad/gate.rs` (a `refused_host` beside `refused`).

**Behaviour:**
- `Actions` and `Run` are gated on `host` (and approval) per message. They do **not** need
  `input`, and do not create uinput devices.
- `Run` executes off the ordered keyboard queue: `tokio::spawn`, reply `Ran`.
- `info!` logs device, action and ok/err.
- On the relay path they ride `PadIn`/`PadOut` like everything else.

**Tests:**
- `host` but no `input`: Actions are answered, and a Frame is refused.
- `input` but no `host`: Run is refused with the host copy.
- A run that fails returns its message in `Ran`.

---

### Task 5: Landscape (client) — invoke `motion-design` + `ui-ux-pro-max` first

**Files:**
- new `crates/client/src/js/pad_orient.js`: the orientation state, the rotation transform, and
  fullscreen with the orientation lock.
- `js/pad_touch.js`: maps pointer coordinates through `W.padOrient.toPad(x, y)` before normalising.
  Reports `Size` in pad (landscape) dimensions.
- `assets/pad.css`: the rotated container.

**Behaviour:**
- **Landscape on, and the viewport upright:** `#wado-trackpad` is rotated 90° (`flip` picks 90° or
  −90°), sized to the viewport's height × width, so the user holds the phone sideways with
  rotation locked.
- **Landscape on, and the viewport already landscape:** no rotation.
- **Landscape off:** today's portrait layout.
- **Fullscreen on:** `requestFullscreen()` then `screen.orientation.lock("landscape")`. If either
  fails, fall back to the rotation and say so once, quietly.
- The rotation applies to touch, the finger rings (brief B's `pad_feel.js` must place rings in the
  rotated frame) and the size sent to the daemon. The pacer and libinput see a landscape pad.
- **The phone keyboard in rotated mode:** the OS keyboard will appear upright, not rotated. So
  while typing in rotated landscape, show the text line upright at the bottom (the rail stays
  rotated), or temporarily un-rotate. Pick the clearer of the two using `ui-ux-pro-max`, screenshot
  both, and record the choice in the report.

**Tests (pad-check):**
- `toPad` for 90° and −90°: moving right as held equals +x on the pad.
- `Size` is landscape when rotated.
- Rotating mid-touch lifts first: `size` changes, then the daemon's resize-lift rule handles it.

---

### Task 6: Rail and quick bar (client) — skills first

**Files:** new `js/pad_rail.js`; `pad_ui.js` (layout switch); `pad.css`.

**Layout:**
- **Landscape:**
  - **Rail on the left**, the pad on the right. Status pill at the top of the rail, and the close
    button at the top right of the pad.
  - `rail` = `keys`: modifiers (Super, Ctrl, Alt, Shift), Esc and Tab, arrows, a Type button.
  - `rail` = `actions`: the `quick` action buttons (icon and short label) from `ActionList`, in the
    user's order.
  - `rail` = `none`: pure pad. The pill and close button float over the pad corners.
  - The rail scrolls vertically if it overflows. Targets ≥ 44 px.
- **Portrait:** layout A as built. When `rail` = `actions`, the quick actions show as a horizontal
  bar above the pad.

**Feedback:**
- A pressed action button gives press feedback (`scale .96`, `--pop` release).
- A running action shows a brief spinner, then a check or an error, ≤ 400 ms total, with the error
  copy in the alert card.

**Tests:**
- The rail renders the `quick` ids in order and skips ids not in `ActionList`.
- A tap sends `{k:"run", id}`.
- A `Ran { ok: false }` shows the message.

---

### Task 7: Settings sheet inside the trackpad (client) — skills first

**Files:** new `js/pad_settings.js`; ⚙ in the pad header (portrait) or the rail (landscape).

**The sheet (bottom sheet in portrait, side sheet in landscape; `--spring` entrance, ≤ 400 ms):**
- **Landscape:** on/off, Fullscreen on/off, Flip.
- **Rail:** Keys / Quick actions / None (a segmented control).
- **Quick actions:** every available action grouped as in `ActionList`, with a switch to include
  it and drag (or ↑/↓ buttons) to order. A device without `host` sees one line with the grant
  copy.
- **Gestures:** each of the four phone gestures with an action picker, or Off.
- **Keyboard opens with the pad:** on/off.

**Persistence:** each change goes through `ConfigSet` to `device.<own key>.trackpad.<field>`, so it
lands in `config.kdl` (ui.kdl) and survives. Mirror it to localStorage for instant load. On open,
read the device's config if the daemon sends it (otherwise the local mirror). Find how the client
receives its own config today (`js/config.js`) and reuse that path.

**Tests:**
- Changing the rail sends a ConfigSet with the right key and value.
- A reload reads it back (pad-check with a stubbed config).

---

### Task 8: Phone gestures (client) — skills first (plan 3)

**Files:** new `js/pad_gestures.js`, sitting before `pad_touch` in the event path. It may swallow a
pointer sequence only per the rules below.

**Gestures (all orientation-aware, using the edges *as held*):**
- **edge-left / edge-right:** a single finger starting within 10 px of that edge and moving inward
  ≥ 40 px within 300 ms. Hold the first 10 px band's touches for at most 60 ms before deciding:
  either forward them (as the touch they are) or claim them. No other touch is ever delayed.
- **four-tap:** four fingers down and all up within 250 ms, movement < 12 px. Do not forward a
  fifth frame. Forwarding the four-finger touch to libinput is acceptable only if no action is
  assigned; when one is assigned, claim it.
- **two-hold:** two fingers still (< 8 px) for 600 ms. On claim, lift those contacts on the laptop
  (send an empty frame) before running the action.
- An unassigned gesture is never claimed: everything is forwarded as today.

**Feedback (motion-design):**
- An edge swipe shows a short accent edge glow following the finger.
- A claim gives a small confirmation pulse with the action's icon (≤ 300 ms).

**Tests (pad-check, fake clock):**
- Each gesture claims only when assigned.
- A three-finger swipe is always forwarded unchanged.
- A two-finger scroll is never claimed as two-hold, because it moved.
- An edge touch that becomes a normal drag is forwarded, with at most 60 ms extra delay on its
  first frame only.

---

### Task 9: Verify on the real machine and hand over

- **Daemon:** `cargo test --workspace`, the `#[ignore]` action tests unsandboxed, then a release
  build and swap (stop the exact pid, relaunch detached with its environ; log at
  `~/.cache/wado/daemon.log`).
- **Client:** `dx build`, then screenshots at 390×844 portrait and 844×390 landscape, plus the
  rotated-upright case, in dark and light. Show the rail in all three modes, the settings sheet,
  and a claimed gesture.
- **TODO.md phone checklist (append):**
  - landscape held sideways with rotation lock on: pointer direction is right
  - quick action runs: workspace next/prev, fullscreen, play/pause, volume
  - each gesture once
  - a three-finger swipe still switches workspace
  - the settings survive a reload
- `PRODUCT.md`/`DESIGN.md`: add the rail and settings-sheet components to DESIGN.md's Components.
