# Trackpad mode: the phone as the computer's touchpad, keyboard and remote

Status: design, **approved by the user** `2026-10-29`
Context: `PRODUCT.md` (device as controller), `DESIGN.md` (visual system), `CLAUDE.md` invariants.

## Intent

The user sits on the couch with the laptop on the TV. They want the phone to be the laptop's
**wireless trackpad and keyboard**, as if it were a Magic Trackpad, plus one-tap **actions**
(fullscreen, workspace, volume, lock, their own commands). The phone shows **no picture**. The
mode must work with **no wado session running**. It drives the computer's own desktop, not a wado
compositor.

What makes it worth having over a cheap Bluetooth trackpad: phone dictation types straight into
the laptop, actions fit the user's compositor, and the phone can carry gestures and buttons of
its own.

## Goals

1. **Touchpad.** The laptop sees a real multi-touch touchpad. Hyprland's own gestures, pointer
   speed, tap-to-click, natural scroll and palm rejection apply unchanged, because libinput
   handles the device like any other touchpad.
2. **Keyboard.** The phone's own keyboard types ordinary text into the focused app, including
   autocorrect, swipe typing, **voice dictation** and non-ASCII text. A sticky modifier row
   (Super, Ctrl, Alt, Shift, Esc, Tab) makes shortcuts one tap per key: tap Super, then 1.
3. **Actions library.** Named actions the user picks without knowing how they run. wado maps each
   one to the mechanism that works on this computer.
4. **Settings page for actions.** Browse the built-ins, add custom ones, and choose which appear
   on the quick bar beside the touchpad.
5. **Phone gestures → actions.** A small set of gestures belongs to wado, not to the laptop. They
   run an action instead of reaching the touchpad, and each is assignable and can be switched off.
6. **First platform coverage:** Hyprland, plus general Linux tools. Other compositors get the
   general actions and the raw input, and the Hyprland-only actions are hidden there.

## Non-goals (this version)

- No picture of the computer on the phone.
- No air mouse (gyroscope), presenter mode or haptics. These are recorded as candidates, not
  built.
- No compositors beyond Hyprland for compositor-specific actions (sway/niri/KDE/GNOME come later).
- No macOS host.

## Anti-goals (failure even if it technically works)

- **Lag.** The cursor visibly trails the finger. This is the user's worst outcome.
- **Bad detection.** This is worse than lag. It covers taps read as drags, scrolls read as
  clicks, a resting palm moving the pointer, and wado's phone gestures stealing a Hyprland swipe
  or the reverse. *(Confirmed by the user: "misreading is what I meant".)*
- A gesture or button that silently does nothing. Unavailable actions are hidden or say why,
  in plain words.

## Decided with the user

- **Layout A.** The whole screen is the trackpad. At the top sit the link pill (true state, plain
  words) and a settings button. A **Keyboard** button opens the phone keyboard, with the sticky
  modifier row and a text line above it, and the pad stays live above the keyboard.
- **B's shortcut rail is optional.** A setting ("Always show quick bar") pins the user's chosen
  actions above the pad even when the keyboard is closed.
- **Shortcuts are actions:** a built-in library plus the user's own, managed on a settings page,
  shown beside the touchpad when the user wants them.
- **Actions don't depend on the compositor** from the user's view. "Fullscreen" is one action;
  wado decides how it runs here.
- **Custom gestures** on the phone run actions, separately from the computer's own gestures.
- **Permission split:**
  - Touchpad and keyboard need a **new `input` grant**, **off by default** for every device, new
    or legacy. Typing into the desktop is terminal-equivalent, so it must never arrive silently
    with `host`. It is granted per device from `wado tui`, `wado allow`, or the pairing checklist
    (the same three places as the other tokens, `gate/grants.rs`).
  - Built-in actions need the device's **`host`** grant.
  - Custom commands defined in **`config.kdl` on the computer** need only **`host`**, since the
    owner wrote them and the phone only triggers them.
  - Custom commands **created from the phone** need **`shells`**, because a free-form command is
    as powerful as a terminal. This extends the `2026-10-02` "binds have no spawn" reasoning.

## Constraints

- **Invariant 1: input never rides behind anything that can head-of-line block it.** Touch and
  key traffic must not share an SCTP association with file transfers or video. It needs its own
  peer connection or channel, the way the file manager has its own (`js/files_link.js`).
- **Trackpad works in every situation (user, `2026-10-29`).** When no direct path exists, it falls
  back to the relay WebSocket. This is a **logged exception to invariant 1** (Decision Log
  `2026-10-29`): it applies to this mode's input only when Direct is impossible, and the link pill
  says "Relayed" in plain words. The user expects it to stay **smooth on any connection**. So the
  relay path shapes its own traffic:
  - motion coalesced newest-wins into a fixed send tick, so a TCP stall delivers one fresh
    position rather than a backlog
  - down/up/keys never coalesced
  - a small host-side smoothing step that turns bursty arrivals into a steady cursor
  Network round-trip time cannot be removed, only hidden. The fallback is measured and its
  number reported, never claimed as "smooth".
- **Trust boundary.** The daemon checks the grant on every input and action message, not only
  when the link opens. A grant revoked from `wado tui` takes effect on the next message.
- **The host-side injection must not need root.** `/dev/uinput` is accessible to the user here
  (in the `uinput` group). On a machine where it isn't, the mode says so in plain words, with
  the one-line fix, and shows no touchpad.
- **Devices are torn down** when the link drops, the mode is left, or the daemon exits, so the
  laptop never keeps a phantom touchpad or a stuck modifier. Every held key is released on
  teardown.
- **Visual system:** `DESIGN.md`, base16 tokens only. The pad is a large surface-1 area.
  Modifiers use the segmented/tile language (an accent fill when latched). The link pill is the
  existing component. Targets are at least 44px. WCAG 2.2 AA on all chrome.
- **Module split** (CLAUDE.md, "atomic module separation"): one job per file, on both the daemon
  and the client.
- **Measure before claiming.** Latency is reported as measured numbers, never asserted.

## How it works (decided parts, and my calls)

### Touch path *(my call; the user approved the outcome, not the mechanism)*

- The phone forwards **raw contacts**: id, phase, x and y in pad-normalised units, plus a
  timestamp. It does **no** tap, scroll or palm interpretation of its own for ordinary touches.
  libinput on the laptop does that, which is the main defence against bad detection.
- The daemon creates a uinput **multi-touch touchpad**: `ABS_MT_SLOT`, `ABS_MT_TRACKING_ID`,
  `ABS_MT_POSITION_X/Y`, `BTN_TOUCH`, `BTN_TOOL_FINGER…QUINTTAP`, `BTN_LEFT`, and
  `INPUT_PROP_POINTER` + `INPUT_PROP_BUTTONPAD`, with a resolution so libinput computes physical
  size. The device's aspect ratio is the phone pad's aspect ratio.
- Motion goes on an **unreliable, newest-wins** channel. Contact down/up go on a reliable one.
  A lost "up" must never leave a finger stuck. The daemon also lifts any contact that is silent
  beyond a short timeout.

### Keyboard path *(my call)*

- **Text** (anything the phone's keyboard commits, dictation included) is sent as **text** and
  typed on the host through a path that handles any Unicode. On Wayland, that is the
  virtual-keyboard protocol (`zwp_virtual_keyboard_v1`, which Hyprland supports) with a keymap
  generated for the string. A uinput keyboard can only type what the current layout maps.
- **Keys and shortcuts** (modifiers, Esc, Tab, arrows, combos) go through a uinput **keyboard**
  as real key codes, so compositor binds such as `Super+1` fire.
- Modifiers are **sticky**: tapping one latches it for the next key, and double-tapping locks
  it. The latch shows as the accent fill.
- *My call:* while a modifier is latched, the **next single character** the phone keyboard
  commits is sent as a **key code** (looked up in the host's active layout) instead of as text,
  then the latch clears. So "tap Super, then 1" works. Characters with no key on the host's
  layout make the latch say so and clear it. Dictation and multi-character commits are always
  text and ignore the latch.

### Actions *(the user decided the model; the catalogue below is my first cut)*

An action has a stable id, a plain-words name, an icon, a group, and per-platform
implementations. The daemon reports which actions are available on this computer (it detects
the compositor and probes for the tools). The client shows only those.

| Group | Actions (first cut) | Mechanism |
|---|---|---|
| Window (Hyprland) | Fullscreen, Close, Float/tile, Pin, Move to workspace N, Go to workspace N, Next/prev workspace, Focus left/right | `hyprctl dispatch …` |
| Media | Play/pause, Next, Previous | existing `host::media` (MPRIS) |
| Sound | Volume up/down, Mute | existing `host::audio` |
| System | Lock screen, Suspend, Screenshot (region/full), Brightness up/down | `loginctl lock-session`, `systemctl suspend`, `grim`/`slurp`, `brightnessctl` |
| Apps | Open terminal, Open browser, Open file manager | `xdg-terminal-exec` / `xdg-open` / desktop entries |
| Keys | Any key combo, e.g. Ctrl+C, Ctrl+V, Alt+Tab | uinput keyboard |
| Custom | The user's own command, or a key combo | see Permission split |

Custom actions:
- **From `config.kdl`:** declared by the owner, synced to the client, runnable with `host`.
- **From the phone:** a key combo needs `host`; a command needs `shells` and is stored on the
  daemon (as `ui.kdl` client edits already are).

### Phone gestures *(the mechanism is my call; the user approved separate gestures)*

To avoid fighting Hyprland, wado's gestures are ones a laptop touchpad never produces, so they
are intercepted on the phone and never forwarded:
- **Edge swipe in** from the left or right screen edge (starting in an 8–12px band).
- **Four-finger tap**, with no movement over the threshold.
- **Long two-finger hold** (~600ms, still).

Each is assignable to any action, or to nothing. *My call:* **all are off by default**, so
nothing can steal a touch until the user assigns it. Gestures that
look like a touchpad gesture (two- or three-finger swipes, pinch) are **never** claimed by wado.
Thresholds live in one module and are tunable.

### Client entry and screens

- **Landing:** each computer gets a **Trackpad** button next to Start. It opens the mode with no
  session.
- **Trackpad screen** (layout A): link pill, settings button, the pad, the Keyboard button, and
  the optional quick bar.
- **Keyboard open:** the modifier row, then the text line, then the phone keyboard, with the
  pad shrunk above.
- **Settings → Trackpad** (a new control-centre page):
  - quick-bar on/off and its actions in order
  - gesture assignments
  - pad options (show click buttons, landscape lock)
  - a link to **Actions**
- **Settings → Actions:** browse by group, add custom (a key combo, or a command if the device
  has `shells`), edit, delete. Actions from `config.kdl` are shown as owned by the computer and
  read-only on the phone.
- Wake lock while the mode is open.

## Success

- On the same Wi-Fi, a finger movement reaches the laptop's cursor fast enough that the user
  doesn't notice trailing. *My call on the number:* measure the input round trip on the phone,
  then target **p90 at or under 30ms one-way-equivalent on LAN**, and report the baseline before
  tuning.
- **Relay fallback measured** on mobile data with Direct blocked: the cursor moves without
  visible stepping or catch-up bursts, and the input round trip is reported next to the LAN
  number.
- **Bad-detection check on a real phone:**
  - 20 taps register as 20 clicks
  - 2-finger scroll never clicks
  - a 3-finger Hyprland swipe always switches workspace and never triggers a wado gesture
  - a resting thumb at the edge doesn't move the pointer
- Dictating a sentence on the phone types it correctly into a text editor on the laptop,
  including punctuation and one non-ASCII character.
- Leaving the mode, killing the phone's browser, or dropping Wi-Fi leaves no stuck key, no
  stuck finger and no leftover input device (`libinput list-devices` before and after).
- Each first-cut action works on the user's Hyprland laptop or is hidden with a reason.

## Build order

The user asked for this **before** the remaining P0 work (`2026-10-29`). Where P0 later changes
trust (pairing by code only, passkeys), the grant checks here must follow it. Keep them in one
place, so that change is a single edit.

## Left to the builder

- The crate and module placement of the uinput devices. `crates/compositor/src/input/gamepad.rs`
  has a raw-ioctl uinput pattern. The new devices belong to the **daemon** (no compositor
  involved), so whether to share that helper is the builder's call, without breaking the
  compositor/server boundary.
- The wire format of the new messages (`wado_protocol`), keeping the relay handshake frozen.
- Whether the mode's peer connection reuses the files-link pattern or generalises it.
- The exact thresholds and the uinput resolution values, tuned on the phone.
- The icon set for actions (the existing `widgets/icon.rs` set first).
