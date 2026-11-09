# Touchpad feel: glass vs a real pad, the settings worth exposing, 3/4-finger gestures, motion

Status: **research, for review.** No product code changed. Written `2026-10-30` for the Touchpad
settings page now being built (virtual-input spec §5.2) and for the next trackpad run.

Asked by the user: *"touchpad still can have more shiny and better animations and its 3 finger and
four finger gesture can be more intuitive. You can first research on how a touchpad surface vs a
glass display surface multi touch experience differ and have a threshold slider or cursor speed etc
built into it."*

**How this was grounded.** The wado code was read (client `js/pad_*.js`, `assets/pad.css`; daemon
`pad/*`, `platform/linux/touchpad.rs`, `uinput.rs`; config `trackpad.rs`, `device_trackpad.rs`).
The **libinput 1.31.3 source** that this laptop's Hyprland links was read from the Nix store, and so
was the **Hyprland 0.56.2 source**. The user's Hyprland config (`~/.config/hypr`) and
`hyprctl descriptions` were checked, and so were the daemon's own `pad: arrivals` logs
(`~/.cache/wado/daemon.log`). Web sources are cited inline and listed at the end. A claim marked
**(reasoned)** has not been measured yet.

---

## 0. Top 10 recommendations

| # | Recommendation | Why (one line) |
|---|---|---|
| 1 | **Fix the phone's frame timing first.** Stamp each frame with the pointer event's `timeStamp`, not `performance.now()` at send. Send one frame per input batch, not one per finger event. | The live log shows the adaptive delay **pinned at its 40 ms maximum** (median of 61 reports) and still starving. That is the largest controllable latency term, bigger than anything the animations can hide. |
| 2 | **Hold a Wi-Fi low-latency lock in the Android app** while the pad is open (`WIFI_MODE_FULL_LOW_LATENCY`). | Phone Wi-Fi power save batches packets. That arrival jitter is what forces the pacer's delay up. |
| 3 | **Cursor speed belongs to libinput, never to the phone.** For the "Pointer speed" slider (0.5–2×), the daemon sets Hyprland's per-device `sensitivity` on `wado-trackpad`. Scaling coordinates or the declared size on the phone also shrinks every libinput threshold measured in mm. | It avoids double acceleration and keeps taps and gestures detected correctly (anti-goal #1). |
| 4 | **Give the pad a way to click without tapping.** The device declares `BTN_LEFT` but never presses it, so libinput turns tap-to-click **off** by default. Only Hyprland's own default turns it on. Add an optional press bar that sends `BTN_LEFT`. | On GNOME, or with tapping off, the phone pad cannot click at all today. |
| 5 | **Mark thumbs and palms as `MT_TOOL_PALM`** from the phone, using contact size and resting position. Declare `ABS_MT_TOOL_TYPE` on the uinput device. | libinput has no pressure or size from wado, so a resting thumb **counts as a finger**: a 3-finger swipe becomes a 4-finger one. `MT_TOOL_PALM` needs no quirk file and no root. |
| 6 | **Let the compositor's native 1:1 gestures run, and make the phone mirror them truthfully.** Lock the hint's axis the way Hyprland does (first ~5 px of accelerated delta, about 0.5 mm). Label it with the computer's real gesture map, and show progress to the commit point with a spring-back when it is cancelled. | Most of the "unintuitive" feel comes from Hyprland firing a direction picked in the first half-millimetre, and from dispatcher gestures that fire on lift with no threshold and no feedback. |
| 7 | **Recommended Hyprland map:** 3 fingers horizontal → `workspace` (1:1 with cancel), 4 up → `special` or overview, 4 horizontal → move window. Avoid `dispatcher` gestures where a 1:1 action exists. | Only the 1:1 actions have cancel and spring-back (`workspace`, `special`, `fullscreen`, `float`, `close`). `dispatcher` fires on lift, always. |
| 8 | **Pair the haptics to thresholds, not to movement.** Gesture direction locked → `GESTURE_START`. Commit crossed → `GESTURE_THRESHOLD_ACTIVATE`, and `…_DEACTIVATE` when crossed back. A tap → `VIRTUAL_KEY` (exists). No tick trains on scroll. | Haptics stand in for the physical click and the gesture "detent" that glass lacks. |
| 9 | **Keep the visible settings few:** two sliders (Pointer speed, Scroll speed), plus switches for tap-to-click, natural scrolling, tap-then-drag / drag lock, click bar and finger marks. Everything else stays in `config.kdl`. | Most "thresholds" are hard-coded in libinput (tap 180 ms / 1.3 mm, gesture boxes in mm). A slider that cannot honestly move them would break the honest-status rule. |
| 10 | **Measure feel with tools that already exist.** Use the daemon's arrivals stats, `libinput record` / `analyze per-slot-delta` / `debug-events --verbose` on the wado device, a scripted constant-speed stroke from the phone, and 240 fps camera input-to-photon timing, compared against the 55 ms just-noticeable-difference (JND) for indirect dragging. | Per CLAUDE.md: measure before fixing, and report what the number said. |

---

## 1. What wado does today (facts from the code)

| Area | Today | Where |
|---|---|---|
| What is sent | Full-state frames `{seq, t, c:[[id,x,y]…]}`, x/y normalised 0..65535, `t = performance.now()` **at send time** | `crates/client/src/js/pad_touch.js` |
| When it is sent (direct link) | Once per microtask after **each** pointer event. Taps are repeated at +20/+60 ms. A resting finger is re-sent every 100 ms | `pad_touch.js` (`soon()` → `queueMicrotask`) |
| Coalesced samples | Not read (`getCoalescedEvents` unused). `pointerrawupdate` unused | `pad_pointer.js` |
| Contact geometry | `PointerEvent.width/height/pressure` not forwarded | `pad_pointer.js` |
| Device | uinput "wado trackpad" (`wado-trackpad` in Hyprland), `BUS_VIRTUAL`, vendor/product 0. `INPUT_PROP_POINTER` + `INPUT_PROP_BUTTONPAD`. 5 slots, `BTN_TOOL_*` up to QUINTTAP, `BTN_LEFT` declared, **no** `ABS_MT_PRESSURE`, `ABS_MT_TOUCH_MAJOR` or `ABS_MT_TOOL_TYPE` | `crates/server/src/platform/linux/touchpad.rs`, `uinput.rs` |
| Physical size | axis = CSS px × 10; resolution = `px-per-mm` × 10 = **60 units/mm** (default `px-per-mm 6`). Not measured per phone | `touchpad.rs`, `crates/config/src/schema/trackpad.rs` |
| Pacing | Frames are replayed on the phone's timeline at **125 Hz**, interpolated, never extrapolated. Delay `adaptive` = median frame interval + p98 lateness, slewed, clamped **8–40 ms** | `crates/server/src/pad/pacer.rs`, `delay.rs` |
| Phone gestures | edge-left/right (10 px band, ≥40 px inward in 300 ms, ≤60 ms hold-back), four-tap (<250 ms, <12 px), two-hold (600 ms, <8 px). All off by default | `pad_gestures.js` |
| Feedback | 1:1 rings (land 160 ms `--pop`, lift 140 ms `ease-in`), halo, pinch connector, count chip at ≥3, scroll/pinch/swipe hints, first-use coach, edge glow, claim pulse | `pad_feel.js`, `pad_hints.js`, `pad_coach.js`, `assets/pad.css` |
| Haptics | Android app only: tap mirror (`<180 ms`, `<10 CSS px`) → `VIRTUAL_KEY`. Claim → `GESTURE_START`. Action → `CONFIRM`/`REJECT` | `native_feel.js`, `android/…/device/Haptics.kt` |
| This user's Hyprland | `gesture = 3, horizontal, workspace`. 3 up/down and all 4-finger directions are `dispatcher, exec …`. `workspace_swipe_distance 300`, `cancel_ratio 0.25`, `min_speed_to_force 10`. Touchpad: natural scroll, `clickfinger_behavior`, `scroll_factor 0.7`, tap-to-click on (Hyprland default) | `~/.config/hypr/{setup.conf, modules/gestures.conf, hyprland/general.conf}` |

### Measured: the pacer is running at its ceiling

These are the daemon's own `pad: arrivals` reports: 61 five-second windows with ≥50 frames, from
Android Chrome, mostly on the direct link, on `2026-10-30`.

| Metric | Median | p90 |
|---|---|---|
| arrival gap p50 (ms) | 5 | 15 |
| arrival gap p90 (ms) | 20 | 40 |
| frames late (per 5 s) | 5 | 46 |
| **delay in use (ms)** | **40 (= `direct-delay-max`)** | 40 |
| starved ticks (per 5 s) | 12 | 45 |

What this says:

- The adaptive delay sits at the maximum the config allows, and still starves. So **40 ms of
  deliberate buffering is added on top of the network** on the direct link.
- **(reasoned)** Gap p50 ≈ 0–5 ms with a 60–120 Hz display is consistent with *one frame per finger
  event*. Each finger's `pointermove` gets its own microtask checkpoint, so N fingers produce N
  frames per vsync, all stamped with nearly the same send time. That alone does not make frames
  late, but it makes the interval and lateness statistics noisy. Phone Wi-Fi power save (bursty
  delivery) is the likely source of the large lateness tail. Neither cause is measured yet; §8 M1/M2
  say how to separate them.
- Why it matters: Deber et al. measured the **just-noticeable latency for indirect dragging at
  55 ms** (tapping 96 ms). Direct touch is far stricter: dragging 11 ms, tapping 69 ms. A touchpad
  is indirect, so it has more headroom, but 40 ms of buffer uses most of it before the network, the
  compositor frame and the display are counted. ([Deber et al., CHI 2015](https://www.tactuallabs.com/papers/howMuchFasterIsFastEnoughCHI15.pdf))

---

## 2. A touchpad surface vs a glass display

| Property | Laptop / Magic touchpad | Phone glass as wado's pad | What it means for wado |
|---|---|---|---|
| **Surface and friction** | Textured glass or Mylar, made for gliding. Finger friction stays fairly constant | Smooth glass with an oleophobic coating. Friction rises with skin moisture, and wet or occluded glass shows stick-slip ([Pasumarty et al.](https://www.researchgate.net/publication/226856388_Friction_of_the_Human_Finger_Pad_Influence_of_Moisture_Occlusion_and_Velocity); [Ayyildiz et al., J. R. Soc. Interface 2019](https://royalsocietypublishing.org/rsif/article/16/156/20190166/87123/Fingerpad-contact-evolution-under)) | Slow, precise strokes judder (stick-slip) and long swipes drag. **Use a slightly higher low-speed gain rather than a lower one** (libinput's sub-7 mm/s deceleration bites hardest here), and keep strokes short. A matte screen protector is a legitimate user tip. **(reasoned)** |
| **Contact sensing** | Capacitive grid made for fingers. Often reports pressure and touch size (Apple, Synaptics) | Capacitive touchscreen. Android/Chrome report `width/height` (touch major/minor) per contact. `pressure` is usually synthetic | libinput gets **no pressure and no size** from wado, so pressure/size thumb and palm detection is off (`evdev-mt-touchpad-thumb.c`, `tp_init_palmdetect_size` needs a quirk). The phone knows contact size and could say "palm" (§3.3). |
| **Size** | Windows PTP minimum **32 × 64 mm**, recommended **≈65 × 105 mm** ([Microsoft](https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-device-integration)) | Landscape pad (rail beside it) ≈ **100–125 × 55–65 mm**. Portrait ≈ 60–68 × 90–110 mm. A Pixel-7-class screen is 65.9 × 146.5 mm, about 6.25 CSS px/mm | **In landscape the phone is a normal-sized touchpad.** Size is not the problem; the declared size must be **true** (§3). Portrait is narrower than libinput's 70 mm edge-palm cut-off. |
| **Sensor rate → app** | 80–140 Hz to the kernel. libinput's thresholds were tuned at a 12 ms (~83 Hz) reference | Digitisers sample faster than the display (often 120–240 Hz per vendor specs). Browsers deliver `pointermove` **once per frame**, aligned to `requestAnimationFrame` ([Chrome](https://developer.chrome.com/blog/aligning-input-events)). Intermediate samples come only via `getCoalescedEvents()`, or `pointerrawupdate` in secure contexts ([W3C Pointer Events](https://w3c.github.io/pointerevents/); [MDN](https://developer.mozilla.org/en-US/docs/Web/API/Element/pointerrawupdate_event)). Android batches `MotionEvent`s per vsync, with history in `getHistorical*` ([Android](https://developer.android.com/reference/android/view/MotionEvent)) | wado reads one sample per frame per finger today. Coalesced samples double the data for free on 120–240 Hz panels, and the pacer interpolates them correctly once `t` is the event's own `timeStamp`. |
| **Physical click / force** | Clickpad: the whole pad presses (`BTN_LEFT`), or haptic force-click | None. Web force is unavailable; Android pressure is unreliable | The phone needs a **substitute**: tap-to-click (a libinput setting), or an on-screen press bar that sends a real `BTN_LEFT`, so libinput's clickfinger / button-areas logic applies. |
| **Haptics** | Physical click, or Apple's Taptic force-click detent | Phone vibration motor: Android predefined effects ([HapticFeedbackConstants](https://developer.android.com/reference/android/view/HapticFeedbackConstants)). `navigator.vibrate` needs sticky user activation and is absent on iOS Safari ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/Navigator/vibrate)) | Haptics stand in for the click and the gesture detent (§7.2). |
| **Palm / thumb** | Palm rests beside the pad. Firmware or libinput reject palms by edge zone, pressure, size and typing | The phone is held. Thumbs rest **on** the glass edge (portrait) or on the sides (landscape), and the OS cancels some palms (`pointercancel`) | libinput's edge palm zone (8 % of width, max 8 mm, only on pads ≥70 mm wide and tagged internal) covers landscape sides only. Portrait has none. Thumbs then **count as fingers** in 3/4-finger gestures (§6.3). |
| **Edges** | Physical bezel. libinput auto-drag-lock fires when a drag ends at the very edge | Rounded or curved edges. **Android gesture navigation** takes inward swipes from the left/right screen edges (Back) and the bottom (Home). Apps may exclude ≤200 dp per edge, and the cap is waived for windows that hide the navigation bar with transient bars ([Android](https://developer.android.com/guide/navigation/gesturenav)) | wado's edge gestures and strokes that start at the edge compete with Android Back. In the *rotated* landscape layout, the pad's edges are the screen's top and bottom, so they compete with **Home**. The native app goes immersive with `BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE` (`ScreenModes.kt`) but sets no exclusion rects. |
| **Hover / lift-off** | Some pads sense proximity. Lift-off is reported by firmware | No hover. Lift-off is binary and immediate | A lost lift is covered by the +20/+60 ms repeats and the 400 ms silence lift (exists). |
| **The device moves** | Fixed in the chassis | Hand-held, sometimes in the same hand that swipes | Stroke direction stays relative to the glass. Rotation must be handled before sending (exists, `pad_orient.js`). |
| **Latency budget** | ~10–25 ms from finger to the compositor | Phone batching (up to one frame) + network (LAN 2–10 ms, relay more) + **pacer delay (8–40 ms)** + pacer tick (≤8 ms) + compositor frame + display | The pacer's delay is the only big term wado fully controls. |

---

## 3. What libinput assumes about a touchpad, and what the declared size changes

### 3.1 The thresholds are in millimetres

libinput converts device units to millimetres using the axis `resolution`. Every decision below is
in **physical mm or mm/s**, read from the 1.31.3 source:

| Decision | Value | Source (libinput `src/`) |
|---|---|---|
| Tap: down→up within | **180 ms** | `evdev-mt-touchpad-tap.c` `DEFAULT_TAP_TIMEOUT_PERIOD` |
| Tap: may move at most | **1.3 mm** | `DEFAULT_TAP_MOVE_THRESHOLD` |
| Tap-and-drag: re-touch within | 160 ms + 20 ms per finger | `DEFAULT_DRAG_TIMEOUT_PERIOD_*` |
| Drag lock (timeout mode) | 300 ms | `DEFAULT_DRAGLOCK_TIMEOUT_PERIOD` |
| Tapping on by default? | **Only if the device has no `BTN_LEFT`** ("If we don't have a left button we must have tapping enabled by default") | `tp_tap_default()` |
| Hold gesture after | 180 ms still (40 ms "quick hold") | `evdev-mt-touchpad-gestures.c` |
| Gesture: count a finger as moving | 1.5 mm, +0.5 mm per finger over 2 (3 fingers: 2 mm) | `tp_gesture_detect_motion_gestures` |
| Gesture: swipe/scroll if the fingers sit within | **40 mm horizontally and 7 mm vertically**, after 150 ms | same, `DEFAULT_GESTURE_SWIPE_TIMEOUT` |
| Pinch timeout | 300 ms | `DEFAULT_GESTURE_PINCH_TIMEOUT` |
| A finger joining or leaving ends the gesture | "A single gesture cannot change the finger count" | [libinput gestures](https://wayland.freedesktop.org/libinput/doc/latest/gestures.html) |
| Edge palm zone | 8 % of width (≤8 mm) each side, **only if width ≥ 70 mm** and the pad is internal. Top 5 % if height > 55 mm. A touch that starts there is palm unless it leaves inward within 200 ms | `tp_init_palmdetect_edge`, `tp_palm_detect_move_out_of_edge` |
| Thumb detection | Only on clickpads ≥ 50 mm tall. Without pressure/size it only suppresses the lowest finger that stays still while others move | `tp_init_thumb` |
| Jump discard | > 20 mm in one event, or +7 mm, normalised to a 12 ms interval | `tp_detect_jumps` |
| Internal or external | `BUS_BLUETOOTH` → external. **Anything else (wado's `BUS_VIRTUAL`) → internal.** Only PS/2 keyboards count as internal for disable-while-typing, so wado's own uinput keyboard does **not** disable wado's pad | `evdev_tag_touchpad`, `10-generic-keyboard.quirks` |
| Delta smoothing for jittery timing | Only `BUS_BLUETOOTH` gets it (50 ms threshold, 10 ms value) | `tp_init_accel` |

### 3.2 What a wrong declared size does

wado declares `px-per-mm 6`. That is about right for phones: about 6.25 dp/mm on a Pixel-7-class
screen, and iOS points are about 6.0/mm. It is not exact, and Android's "Display size" setting
moves it by ±20 %.

Suppose the declared pad is **k× larger** than the real glass (k > 1, meaning `px-per-mm` is too
small, or a speed slider was implemented by inflating the size):

- **Pointer:** k× faster at a given finger speed. Acceleration also starts k× sooner, because
  libinput sees k× the mm/s.
- **Taps:** the tolerance drops to 1.3/k mm of real finger travel. At k = 2 it is 0.65 mm, which
  glass stick-slip easily exceeds, so **taps start failing**.
- **Gestures:** the 40 × 7 mm swipe box shrinks to 40/k × 7/k real mm, so more ambiguous starts
  are read as pinches.
- **Palm edge:** the zone shrinks to 8/k real mm, and the 70 mm width test passes on smaller pads.
- **Jumps:** a fast real flick can cross 20/k mm per 12 ms and be **discarded as a jump**.

k < 1 does the opposite: the pointer gets slower, taps more forgiving, gestures more swipe-biased.

**Conclusion:** the declared size should be the **truth**, and speed should be changed only where
libinput expects it, through the speed setting. If a fallback must scale the size (a compositor
with no per-device speed), keep k within **0.8–1.25** and label it approximate.

**Getting the truth:** the Android app can compute it from `DisplayMetrics.xdpi/density` (dp per
mm) and send it with `size`. The web client can offer a one-time "match this bar to a bank card"
calibration (85.6 mm), or keep 6.

### 3.3 Palm and thumb marking without root

libinput honours `ABS_MT_TOOL_TYPE = MT_TOOL_PALM` whenever the device advertises
`ABS_MT_TOOL_TYPE` (`tp->palm.use_mt_tool`, `evdev-mt-touchpad.c:3511`). A palm touch is not an
active touch: it moves nothing, taps nothing, and is not counted in gestures. This is the only
palm/thumb channel wado can use without installing a libinput quirk file, which would need root. See
also [libinput palm detection](https://wayland.freedesktop.org/libinput/doc/latest/palm-detection.html).

What the phone would classify **(thresholds to be measured on the user's phone, §8 M6)**:

- **Big contact:** `PointerEvent.width/height` above a threshold, starting near 12 mm, calibrated
  by logging index-finger and thumb contacts.
- **Resting at the edge:** a contact that lands in the outer band *as held* and stays still
  (< 1 mm) while other contacts move.
- **Never the only finger:** a lone contact is always a finger. That keeps "bad detection" from
  creating a dead pad.

---

## 4. Pointer feel

### 4.1 libinput's touchpad curve (adaptive, speed 0)

From `filter-touchpad.c`. Input is normalised to 1000 dpi, so 39.37 units per mm. Then `factor ×
0.2968` (`TP_MAGIC_SLOWDOWN`) × `speed_factor(s)`, with a threshold of 130 mm/s.

| Finger speed | Factor | Host px per mm of finger |
|---|---|---|
| < 7 mm/s | min(0.9, 0.1·v + 0.3) → 0.3–0.9 | 3.5–10.5 (deliberate slow-down for sub-pixel precision) |
| 7–130 mm/s | 0.9 (plateau) | **10.5** |
| 260 mm/s | 1.55 | 18.1 |
| ≥ 520 mm/s (cap at 4 × threshold) | 4.8 | 56 |

The speed setting multiplies the whole curve: `speed_factor(s) = (s+1)^2.38 · 0.95 + 0.05`. The
`flat` profile instead uses `max(0.005, 1+s) × 0.2968`, which gives 11.7 px/mm at s = 0, with no
curve. Two-finger scrolling is never accelerated: 0.9 × 0.2968 → 10.5 px/mm × the compositor's
`scroll_factor`. libinput documents the touchpad curve as the mouse curve "vertically squashed"
([pointer acceleration](https://wayland.freedesktop.org/libinput/doc/latest/pointer-acceleration.html)).

So, on a true-size **110 mm landscape pad**, a calm stroke across the pad moves the pointer about
1150 px, and a flick across it reaches the far edge of a 4K screen. That is the same as a laptop
pad, which is the point.

### 4.2 How to expose speed and acceleration honestly

- **No acceleration on the phone, ever.** The phone sends absolute contacts. libinput already
  accelerates, so anything done on the phone stacks on top of it (double acceleration). It would
  also break absolute mapping: a finger at the glass edge must reach the device edge.
- **"Pointer speed" slider, 0.5×–2×, default 1×**, mapped to libinput speed by inverting
  `speed_factor`: `s = ((m − 0.05)/0.95)^(1/2.38) − 1`.

  | Shown | 0.5× | 0.75× | 1× | 1.5× | 2× |
  |---|---|---|---|---|---|
  | libinput speed | −0.27 | −0.12 | 0 | +0.19 | +0.35 |

  Applied **per device** in Hyprland, so the laptop's own pad is unaffected. Hyprland 0.56 has
  per-device `sensitivity`, `accel_profile`, `scroll_factor`, `natural_scroll`, `tap-to-click`,
  `drag_lock` and `clickfinger_behavior` (`config/legacy/ConfigManager.cpp` `addSpecialConfigValue("device", …)`).
  At runtime: `hyprctl keyword 'device[wado-trackpad]:sensitivity' <s>`
  ([Hyprland #7080](https://github.com/hyprwm/Hyprland/issues/7080)). `keyword` only works with the
  legacy hyprlang config; a Lua config needs `hyprctl eval` (`debug/HyprCtl.cpp` `dispatchKeyword`).
  **Verify on the live machine before relying on it.**
  - On other compositors (GNOME, KDE), speed is a global or desktop-specific setting. Either hide
    the slider with a reason, or fall back to the 0.8–1.25 size scaling labelled "approximate".
- **"Faster flicks go further" switch** (acceleration on = `adaptive`, off = `flat`), default on,
  per device the same way.
- **What other systems do** is consistent with leaving it to the host. Windows, macOS and Xorg all
  use velocity-based transfer functions, and their defaults beat a constant gain by up to 24 % in
  pointing performance ([Casiez & Roussel, UIST 2011, "No more bricolage!"](https://gery.casiez.net/publications/UIST2011-NoMoreBricolage.pdf)).
  The phone should not reinvent one.
- The virtual-input spec's caption "Framework still adds its own acceleration" becomes
  unnecessary once speed *is* the host's setting. Replace it with "Changes only this phone's
  touchpad on Framework."

---

## 5. Thresholds: who owns each one, and which to expose

| Threshold / behaviour | Owner | Can wado change it? | Expose? | Default | Range |
|---|---|---|---|---|---|
| Pointer speed | libinput, via the compositor, per device | Hyprland: yes (`device[wado-trackpad]:sensitivity`) | **Slider** "Pointer speed" | 1× | 0.5–2× |
| Acceleration | libinput, via the compositor | Hyprland: yes (`accel_profile`) | Switch "Faster flicks go further" | on | on/off |
| Tap-to-click | libinput, via the compositor | Hyprland: yes (`tap-to-click`) | Switch "Tap to click" | on | — |
| Tap timeout / move (180 ms / 1.3 mm) | libinput, hard-coded | Only indirectly, through the declared size (do not) | **No** — say "set by the computer" | — | — |
| Tap-then-drag; drag lock | libinput, via the compositor | Hyprland: `tap-and-drag`, `drag_lock` 0/1/2 | Switch "Tap, then drag" + "Keep holding after you lift": Off / Briefly / Until you tap | on; **Briefly** (timeout) on a phone, whose small surface needs re-grips **(reasoned)** | — |
| Two-finger scroll start | libinput (gesture box, timeouts) | No | No | — | — |
| Scroll speed | compositor (`scroll_factor`, per device) | Hyprland: yes | **Slider** "Scroll speed" | 1× (the host's touchpad value when set) | 0.25–2× |
| Natural scrolling | compositor, per device | Hyprland: yes | Switch | the host's touchpad value | — |
| Edge scrolling | libinput (2-finger is the default on multi-touch pads) | — | No | — | — |
| Palm/thumb rejection | libinput (edges, `MT_TOOL_PALM`) + wado phone classification | wado: yes, via `MT_TOOL_PALM` | Switch "Ignore resting thumbs" | on | — |
| Pinch threshold | libinput (1.5 mm + timeouts), Hyprland (`|scale−1| ≥ 0.1` to start) | No | No | — | — |
| 3/4-finger swipe distance, commit, flick | **Hyprland, global** (`gestures:workspace_swipe_*`, per-gesture `scale`) | Only host-wide, which also changes the laptop's pad | Not on the phone page. If anywhere, under "On Framework" (`settings` grant) with that caption | Hyprland's | — |
| wado's own phone gestures (edge 40 px/300 ms, four-tap 250 ms, two-hold 600 ms) | wado phone | yes | Segmented **"Gesture distance: Short / Medium / Long"** (edge-in 28 / 40 / 56 px; two-hold 450 / 600 / 800 ms) — this is the "threshold slider" the user asked for, and it is honest because wado owns these | Medium | — |
| Pacer (smoothing, rate, delay) | wado daemon | yes | No (config.kdl and the host page only) | adaptive; see §9 P0-2 | — |
| Declared pad size (`px-per-mm`) | wado daemon | yes | Advanced: "Pad size: Automatic / Calibrate…" | automatic (native dpi) else 6 | 5–8 |

**Sliders: only two** (Pointer speed, Scroll speed). Everything else is a switch or a three-way
segmented control. The page should say plainly which choices live on the computer:
"Saved for this phone, applied by Framework." This is the honest-status rule.

**Apply path (my call, for review):** the phone saves `device.<key>.trackpad.{speed, accel,
scroll-speed, natural, tap, tap-drag, drag-lock, click-bar, palm}` through the existing
`ConfigSet`. When the daemon creates the uinput device, it applies them to Hyprland. It re-applies
them after Hyprland reloads its config (the reload resets dynamic keywords), which the daemon sees as
the `configreloaded` event on Hyprland's event socket.

---

## 6. 3- and 4-finger gestures

### 6.1 What other systems do

| System | 3 fingers | 4 fingers | Tracking |
|---|---|---|---|
| macOS ([Apple](https://support.apple.com/en-us/102482)) | Drag (optional), tap = Look up | Up = Mission Control, down = App Exposé, left/right = full-screen apps. Thumb + 3 pinch = Launchpad, spread = Show Desktop | 1:1 with momentum |
| Windows PTP ([Microsoft](https://support.microsoft.com/en-us/windows/touchpad-gestures-for-windows-a9d28305-4818-a5df-4e2b-e5590f850741)) | Up = Task View, down = desktop, left/right = switch apps. Customisable | Left/right = switch desktops | 1:1 on Windows 11 |
| GNOME 40+ ([GNOME 40](https://forty.gnome.org/); [OMG Ubuntu](https://www.omgubuntu.co.uk/2021/03/gnome-40-new-features)) | Left/right = workspaces, up/down = overview / app grid | — | 1:1 with spring settle |
| KDE Plasma 6 ([EndeavourOS forum](https://forum.endeavouros.com/t/kde-plasma-6-wayland-touchpad-gestures-proper-ones/52095)) | Left/right = virtual desktops | Up = Overview, down = Grid | 1:1. Not configurable in the GUI |
| Hyprland ≥ 0.51 ([0.51 notes](https://hypr.land/news/update51/)) | Anything: `gesture = <n>, <dir>, <action>` | same | **Depends on the action** (below) |
| libinput itself ([gestures](https://wayland.freedesktop.org/libinput/doc/latest/gestures.html)) | Swipe/pinch/hold events. "It is up to the caller to interpret the gesture" | same | — |

### 6.2 How Hyprland 0.56 turns libinput gestures into actions (source-read)

- **Direction is chosen almost at once.** Hyprland sums libinput's *accelerated* swipe deltas, and
  as soon as `|Σx|` or `|Σy|` reaches **5 px** it picks the dominant axis and sign and locks the
  gesture to it (`managers/input/trackpad/TrackpadGestures.cpp:129`, comment: *"5 was chosen
  because I felt like that's a good number"*). At the plateau gain, 5 px is about **0.5 mm** of
  finger travel after libinput declares the swipe. A slightly diagonal start on glass, which is
  common with staggered landings and stick-slip, picks the wrong row of the gesture map.
- **`dispatcher` gestures fire on lift, unconditionally.** `DispatcherGesture::end` runs the
  dispatcher; `begin`/`update` are empty. There is no threshold, no cancel and nothing on screen
  while the fingers move. This user's 3 up/down (`wsgrid`) and all four 4-finger directions are
  dispatchers, which explains much of "the 3/4-finger gestures are not intuitive".
- **1:1 actions with cancel:**
  - `workspace` follows the fingers, with `workspace_swipe_distance` px per workspace. It commits
    when the lift is past `cancel_ratio`, or when the **mean |delta| per event** is at least
    `min_speed_to_force`.
  - `special` (150 px, cancels below 30 %), `fullscreen` and `float` (250 px, below 20 %) and
    `close` (200 px, below 20 %) animate with the fingers and settle or spring back
    (`gestures/*.cpp` `MAX_DISTANCE`, `COMPLETION`).
- **The flick threshold depends on wado's pacer rate.** `min_speed_to_force` is "px per timepoint",
  and libinput emits one swipe update per frame the pacer writes. At `rate-hz 125`, this user's
  `10` means about 1250 px/s, roughly 120 mm/s of finger speed. At 60 Hz it would be about 57 mm/s.
  **Changing `rate-hz` changes how easily a flick commits.** Keep 125, or document the coupling.
- Pinch starts once the scale moves 0.1 from 1.0.

### 6.3 Phone-specific reasons 3/4-finger gestures misfire

1. **A resting thumb counts.** It has no pressure or size, there is no edge palm zone in portrait,
   and in landscape only the outer 8 mm is covered. Three fingers plus a thumb becomes a 4-finger
   gesture, or adding the thumb mid-swipe ends the gesture. Fix: `MT_TOOL_PALM` (§3.3).
2. **Staggered landing on glass:** the first finger moves the pointer before the others land.
   libinput's 100 ms switch timeout absorbs most of it. Watch for it in `debug-events`; do not
   delay single-finger motion to fix it, because that is the lag anti-goal.
3. **Direction locked at about 0.5 mm** (§6.2). The phone cannot change Hyprland's rule. It can
   make the result *visible early* and teach a clean start.
4. **Dispatcher gestures** have no cancel and no feedback (§6.2).
5. **A wrong declared size** distorts the 40 × 7 mm swipe box (§3.2).
6. **Android Back/Home edge gestures** steal strokes that start at an edge (§2).

### 6.4 Where the gesture should run

The spec's rule holds: **forward raw contacts, and let the compositor's native gestures run**.
That is what gives 1:1 tracking, compositor animations and the user's own map. wado's own gestures
stay limited to motions a touchpad never produces (edge-in, four-tap, two-hold). They stay off by
default and claimed only when assigned (exists).

What wado adds is a **truthful mirror on the phone**, explained in §6.6.

### 6.5 Recommended gesture map

**Hyprland (written as config the user can paste. wado does not edit the compositor's config files.)**

| Gesture | Action | Why |
|---|---|---|
| 3 fingers, horizontal | `workspace` | 1:1, cancel, flick. Already this user's setup |
| 3 fingers, up | `special` (scratchpad), or the user's overview plugin if it offers a 1:1 gesture | 1:1 with cancel at 30 % |
| 3 fingers, down | `special` (close) — or nothing | Paired with up |
| 4 fingers, up / down | `fullscreen` toggle (1:1, cancels below 20 %) or the overview | Avoid `dispatcher` where a 1:1 action exists |
| 4 fingers, horizontal | move window to the next/previous workspace (`dispatcher`, fires on lift) | No 1:1 equivalent; the phone shows "Lift: Move window →" so the user knows |
| 3-finger pinch in | `close` (1:1, 1 s timeout) — optional, off by default | Destructive: opt-in |
| 2-finger pinch | applications (zoom) | Never mapped by wado |
| 4-finger tap | wado phone gesture (an action), off by default | libinput taps stop at 3 fingers |

`gestures { workspace_swipe_distance 300; workspace_swipe_cancel_ratio 0.25–0.35;
workspace_swipe_min_speed_to_force 10 (at 125 Hz) }`. This user's values are already good for a
110 mm pad: about 28 mm of finger travel per workspace, committing at about 7 mm.

For **this user's grid** (3 up/down = `wsgrid` dispatchers), keep them. The phone shows the locked
direction and "Lift: row up" (§6.6), which removes the surprise without wado taking the gesture.

**Generic Linux:**
- GNOME: 3 horizontal = workspaces, 3 up = overview. Built-in, 1:1, nothing to configure.
- KDE Plasma 6: 3/4 horizontal = desktops, 4 up/down = Overview/Grid.

wado's job there is only correct device properties and the mirror.

### 6.6 How the phone teaches it (hints, coach)

1. **A host gesture map message.** The daemon sends the phone `{fingers, direction, label, kind:
   "follows" | "on-lift", distance_px?, commit_ratio?}`.
   - Built from `hyprctl getoption gestures:*` (readable today) plus a best-effort parse of
     `gesture =` lines in the config files. Hyprland has no `hyprctl` command that lists gestures
     (checked: `debug/HyprCtl.cpp` registers none).
   - When unparseable, fall back to the generic labels ("3 fingers →").
   - On GNOME or KDE, the built-in defaults above.
2. **Mirror the axis lock.** `pad_hints.js` today recomputes the axis from the cumulative centroid
   on every frame (`axisOf`), so it can flip after Hyprland has already locked. Lock it once
   centroid travel passes about 0.5 mm in device terms (≈3 CSS px at 6 px/mm), as Hyprland does.
3. **For `follows` gestures:** show progress to the commit point. Estimate host px as finger mm ×
   10.5 × speed factor (§4.1), divided by `distance_px`. Mark the commit notch at `commit_ratio`.
   The phone estimate is a guide, not a promise, so its label never says "will switch", only the
   action's name.
4. **For `on-lift` gestures:** at lock, the badge says "Lift: <label>". That is honest, because
   Hyprland will fire it whatever happens next.
5. **The coach gets a fourth scene,** "3 and 4 fingers", drawn from the host map ("3 fingers
   sideways: workspaces"). It reappears once after the map changes.
6. **Hint for a clean start:** if the first 0.5 mm is diagonal (|dx|≈|dy|) and the swipe then
   continues on the other axis, show "Start the swipe straight" once per session.

---

## 7. Motion and haptics spec

Binding rules (DESIGN.md "Do's and Don'ts"):

- Two curves only: `--spring` `cubic-bezier(.32,.72,0,1)` and `--pop` `cubic-bezier(.34,1.56,.64,1)`.
- Tokens: `--dur` 220, `--dur-sheet` 480, `--dur-hero` 520, `--dur-press` 120, `--stagger` 40.
  Reduced-motion values are 120 / 200 / 160 / 120 / 0.
- Exits run at about 80 % of the entrance. An opacity-only exit may use the accelerate curve
  `cubic-bezier(.3,0,1,1)`, ≤ 180 ms.
- **The trackpad surface is direct manipulation:** it follows the pointer 1:1 with no easing. The
  curve applies only on release or a discrete jump.
- Animate `transform`/`opacity` only.

### 7.1 Elements

| Element | Trigger | Motion | Timing | Reduced / off |
|---|---|---|---|---|
| Contact ring | land | scale .6→1, opacity 0→1 | `--dur-press` `--pop` (today 160 ms: align to the token) | appears, no scale |
| Contact ring | move | `translate` to the finger, once per rAF, **no transition** | 1:1 | same |
| Contact ring | lift | scale 1→1.4, fade | 140 ms **accelerate** curve (today `ease-in`, which is not a DESIGN curve) | removed |
| Ring "resting" state | phone classified the contact as a thumb/palm | dashed outline, opacity .45 | cross-fade `--dur-press` | instant |
| Finger badge (≥3) | count reached | pops in at the centroid. After the axis lock it shows the host label: "3 · Workspaces →" or "4 · Lift: Overview" | `--dur-press` `--pop`; label swap 120 ms opacity | no pop |
| Swipe track (`follows`) | axis locked | a 2 px rail along the axis from the start centroid; the head follows the centroid 1:1; the fill is the progress; a notch at the commit ratio | head 1:1; notch `--pop` 120 ms on crossing | static fill bar |
| Commit (lift past the notch) | lift | the head travels on ~24 px and fades; the badge shows a check and the label for 400 ms | `--dur` `--spring`, then the accelerate fade ≤180 ms | badge only |
| Cancel (lift short) | lift | the track springs back to its origin and fades | `--dur` `--spring` + accelerate fade | fade only |
| `on-lift` gesture | lift | same as commit (it always fires) | as commit | badge only |
| Pinch | 2 fingers spreading/closing | connector 1:1 (exists) + a ring at the midpoint scaled by the live ratio | 1:1 | connector only |
| Scroll | 2 fingers moving together | chevrons trailing (exists; ambient loop = timekeeper) | 720 ms loop (exists) | static chevrons |
| Edge glow / claim pulse | wado gesture | exists | ≤300 ms | — |
| Coach scenes | first use, map changed | exists + scene 4 | linear loop | stops on one frame |
| Settings preview: pointer demo | a slider moved | the mini pointer travels `finger mm × gain(speed)`, computed from §4.1 | the slider thumb 1:1; the demo pointer `--spring` `--dur` | jump to the end |

**Frame budget (120 Hz phone: 8.3 ms per frame).**

- Pointer handlers stay ≤ 0.5 ms and do no layout reads (`pad_pointer.js` already caches the rect
  once per frame).
- Paint once per rAF.
- No `querySelector` in paint: cache the nodes, which `pad_hints.js` does not do today.
- At most 5 rings + 4 overlays. No animated `filter`/`box-shadow`/`backdrop-filter` (DESIGN: no blur
  on coarse pointers).
- Pictures never delay the send: the send happens first, the pictures in a microtask after (exists).

### 7.2 Haptics (Android app; web fallback `navigator.vibrate`, none on iOS)

| Event | Native kind → constant | Web fallback | "Feel in the hand" Light / Medium / Strong |
|---|---|---|---|
| Tap recognised (mirror of libinput's rules) | `tap` → `VIRTUAL_KEY` (exists) | 10 ms | – / ✓ / ✓ |
| 3/4-finger axis locked | `claim` → `GESTURE_START` (API 30) | 8 ms | – / ✓ / ✓ |
| Commit notch crossed / crossed back | new `threshold_on`/`threshold_off` → `GESTURE_THRESHOLD_ACTIVATE` / `…_DEACTIVATE` (API 34; fallback `CLOCK_TICK`) | 6 ms / — | ✓ / ✓ / ✓ |
| Gesture committed | `GESTURE_END` (API 30) | — | – / – / ✓ |
| wado action ran / refused | `CONFIRM` / `REJECT` (exists) | 12 ms / 30–30–30 | ✓ / ✓ / ✓ |
| Drag lock engaged | `DRAG_START` | 10 ms | – / ✓ / ✓ |
| Scroll, pointer motion | **none** — continuous ticks feel cheap and drain the battery | — | — |

**Tap mirror alignment.** `native_feel.js` and `TapFeel.kt` treat a tap as `<180 ms` and
`<10 CSS px`. libinput uses 1.3 mm, which is **7.8 CSS px at `px-per-mm 6`**. Use
`1.3 × px-per-mm`, so the phone never ticks for a tap the computer did not take.

---

## 8. Measurement plan

All numbers are reported as measured, next to the hypothesis they test.

| # | What | How | Pass |
|---|---|---|---|
| M1 | Phone send timing | Dev flag in `pad_touch.js`: per sent frame, log `send − event.timeStamp`, frames per vsync, the number of coalesced samples, and the finger count. 30 s of 1-, 2- and 3-finger strokes | ≤1 frame per vsync per batch; send lag p90 < 2 ms |
| M2 | Arrival jitter and pacer cost | The existing `pad: arrivals` log (gap p50/p90, late, delay, starved), before/after each change, on LAN Wi-Fi with the Android app vs Chrome, with the Wi-Fi lock on/off | Delay median ≤ 24 ms; starved ≤ 2 per 5 s |
| M3 | What libinput gets | `sudo libinput record` on the `wado trackpad` event node during scripted strokes. Check event intervals (125 Hz ±), then `libinput analyze per-slot-delta`. The tools are at `/nix/store/…-libinput-1.31.*-bin/bin`; reading `/dev/input` needs root or the `input` group, and this user has neither | 8.0 ms ± 0.5 ms intervals; no "jump discarded" |
| M4 | What libinput decides | `sudo libinput debug-events --verbose`: tap/palm/gesture decisions ("palm: touch N …", gesture begin/cancel) for the recognition tests | Matches intent in M6 |
| M5 | Pointer smoothness on the host | `wev` (or polling `hyprctl cursorpos`) during a **scripted constant-speed stroke** injected through `W.padTouch.input` (dev-only): 100 mm/s, 60 mm, 20 repeats. Compute the velocity coefficient of variation | CV < 10 % **(target to confirm)** |
| M6 | Recognition | Spec list (20 taps → 20 clicks; 2-finger scroll never clicks; 3-finger swipe always switches). Plus: 20 horizontal 3-finger swipes → wrong-axis count; 3-finger swipe with a resting thumb (portrait and landscape); flick commits at 125 Hz; thumb and index `width/height` histograms for the palm threshold | 0 wrong-axis in 20 when started straight; 0 thumb-caused misfires |
| M7 | Glass to photon | A 240 fps phone camera filming finger and laptop screen. 20 stroke starts and 20 stops; count frames between finger and cursor movement. Report p50/p90, LAN and relay | Indirect-drag JND 55 ms as the reference ([Deber 2015](https://www.tactuallabs.com/papers/howMuchFasterIsFastEnoughCHI15.pdf)) |

Never add browser and daemon times together: their clocks are not synchronised (CLAUDE.md hazard).
M7 is the only end-to-end number.

---

## 9. Prioritised change list

### P0: correctness and latency (measure M1/M2 before and after each)

| # | Change | Files / modules | Expected effect | Risk |
|---|---|---|---|---|
| P0-1 | Stamp `t` with `PointerEvent.timeStamp`. Flush once per input batch on the direct link (rAF right after aligned input; microtask fallback while hidden). Optionally send each `getCoalescedEvents()` sample as its own stamped frame | `crates/client/src/js/pad_touch.js`, `pad_pointer.js`; pad-check tests | Half or fewer frames with multi-finger; true intervals; lower measured lateness → lower adaptive delay **(reasoned)** | This is the send path: a bug loses taps. Keep the +20/+60 ms repeats and the per-landing immediate send; cover with `scripts/pad-check.mjs` |
| P0-2 | Retune the pacer after P0-1/P0-3: try p95 lateness and `direct-delay-max` 24–32 | `crates/server/src/pad/delay.rs`, `crates/config/src/schema/trackpad.rs` defaults; Decision Log entry | −8 to −16 ms on the direct link | More starved ticks = micro-stutter; decide by M2/M5 |
| P0-3 | Android: `WifiManager` low-latency lock while the pad is open. Read `PointerInputChange.historical`. Exclusion rects over the pad's edges (cap waived because the app hides the bars transiently) | `android/…/pad/PadLink.kt`, `ui/pad/PadScreen.kt`, `device/ScreenModes.kt` | Less arrival jitter; no accidental Back/Home | Battery while open (foreground only); test on the user's phone |
| P0-4 | Speed, acceleration, scroll speed, natural scroll, tap, drag lock applied **per device in Hyprland** by the daemon, never on the phone. Re-apply on `configreloaded` | New `crates/server/src/platform/linux/hypr_device.rs` (one job) using the `actions/hypr.rs` instance signature; `crates/config/src/schema/device_trackpad.rs` new fields; protocol prefs additive | Honest, no double acceleration; the slider the user asked for | Lua configs need `eval`; hide the controls on other compositors with a reason |
| P0-5 | Click without tapping: additive `{k:"button", down}` → `BTN_LEFT` on the touchpad device; optional press bar on the pad, auto-offered when the host has tap-to-click off | `crates/protocol/src/pad.rs`, `wado_platform::input::TouchInjector`, `platform/linux/touchpad.rs`, `js/pad_ui.js` + a new `js/pad_click.js` | Works on GNOME/KDE out of the box; libinput clickfinger gives right-click with 2 fingers | The bar's own touch must not become a contact; the bar costs pad height in portrait |

### P1: gestures that read right

| # | Change | Files | Effect | Risk |
|---|---|---|---|---|
| P1-1 | `MT_TOOL_PALM`: declare `ABS_MT_TOOL_TYPE`; the phone classifies resting thumbs and large contacts and sends an additive `palm: [ids]` (the `Contact` tuple stays as it is) | `touchpad.rs` (`slot_events` emits the tool type per slot), `protocol/pad.rs`, `pad/bin.rs`, `js/pad_touch.js` + a new `js/pad_palm.js` | Thumbs stop turning 3-finger swipes into 4 | Misclassifying a finger as a palm makes a dead finger. Never mark a lone contact; thresholds from M6 |
| P1-2 | Host gesture map → phone; hints lock the axis like Hyprland, show progress and the notch for `follows`, "Lift: …" for `on-lift`; coach scene 4 | New `crates/server/src/pad/host_gestures.rs`; `protocol/pad.rs`; split `js/pad_hints.js` (classify vs swipe track) per the module rule; `pad_coach.js` | The user knows what lifting will do; no surprise rows | Parsing the config is best-effort; label the fallback generic |
| P1-3 | Haptic pairing per §7.2; Light/Medium/Strong gate | `native_feel.js`, `android/…/device/Haptics.kt` (add `threshold_on/off`, `gesture_end`, `drag`), `pad_hints` emits the events | Glass gets a "detent" | Over-buzzing; Medium default |
| P1-4 | Tap-mirror thresholds = libinput's (`1.3 × px-per-mm`) | `native_feel.js`, `android/…/pad/TapFeel.kt` | No false ticks | none |
| P1-5 | True pad size: the native app sends dp/mm from `DisplayMetrics`; the web offers optional card calibration; per-device `px-per-mm` override | `device_trackpad.rs`, `touchpad.rs` reads the per-device value, `pad_settings.js` | Every libinput mm threshold becomes true | Changing the size recreates the device (lifts the fingers): apply only at open |

### P2: shine

| # | Change | Files | Effect | Risk |
|---|---|---|---|---|
| P2-1 | Ring resting state, swipe track, commit/cancel spring, badge labels (§7.1); ring land/lift timings aligned to the tokens | `pad_feel.js`, `pad.css`, a new `js/pad_track.js` | "Shiny but calm" | Paint cost; keep within the §7.1 budget |
| P2-2 | Settings page: the two sliders + switches (§5), a live preview using the §4.1 curve, a "Decided by Framework" note only for what wado cannot set | Settings page being built (`ui/computer/input/touchpad.rs` hosting `pad_settings.js`) | The tuning the user asked for, honestly | none |
| P2-3 | "Gesture distance" segmented control for wado's own gestures | `pad_gestures.js` (thresholds from prefs), `device_trackpad.rs` | Tunable edge/hold gestures | none |
| P2-4 | Dev-only scripted stroke injector for M5 | `js/pad_touch.js` behind a debug flag, or `scripts/pad-check.mjs` | Repeatable measurements | Must never ship enabled |

**Considered and rejected:**

- Declaring `BUS_BLUETOOTH` to get libinput's delta smoother. It would also tag the pad external,
  turning off edge-palm zones; the pacer already gives regular timestamps.
- Phone-side acceleration or speed: double acceleration, and it breaks the mm thresholds.
- Delaying the first finger to coalesce staggered landings: adds lag, the worse anti-goal.

---

## 10. Open questions for the user (ambiguity → question, per CLAUDE.md)

1. **May the daemon change Hyprland's per-device options at runtime** (`hyprctl keyword
   device[wado-trackpad]:…`)? It touches only the wado pad, not the laptop's pad and not any config
   file. The values live in wado's `config.kdl`.
2. **Should a press bar exist, and should it be on by default** when the computer has tap-to-click
   off? It costs pad height in portrait.
3. **Should 3-finger vertical swipes on this machine become 1:1** (e.g. `special`)? Or keep the
   `wsgrid` dispatchers and rely on the "Lift: …" mirror? wado taking them over would contradict
   the spec's "never claim a 3-finger swipe".
4. **Should global gesture distance and flick settings appear on the phone at all**, under "On
   Framework" with the `settings` grant? They also change the laptop's own pad.
5. **Default drag lock on the phone:** Off (libinput default), or "Briefly" (timeout), because the
   phone is small?

---

## Sources

**Primary source code, read locally:**

- libinput 1.31.3 (`/nix/store/pf9wambhh1q5qyq2r2vq2aim126j5cfn-source/src`): `filter-touchpad.c`,
  `filter-touchpad-flat.c`, `evdev-mt-touchpad.c`, `evdev-mt-touchpad-tap.c`,
  `evdev-mt-touchpad-gestures.c`, `evdev-mt-touchpad-thumb.c`, `evdev-mt-touchpad-buttons.c`,
  `evdev.c`, `libinput.h`. Upstream: https://gitlab.freedesktop.org/libinput/libinput
- Hyprland 0.56.2 (`/nix/store/xm2laz7bplbf2wnsx9p4iaxcy95gmv1k-source/src`):
  `managers/input/trackpad/TrackpadGestures.cpp`, `gestures/*.cpp`,
  `managers/input/UnifiedWorkspaceSwipeGesture.cpp`, `debug/HyprCtl.cpp`,
  `config/legacy/ConfigManager.cpp`. Upstream: https://github.com/hyprwm/Hyprland
- `hyprctl descriptions` on this machine (defaults of `gestures:*` and `input:touchpad:*`).

**Documentation and papers:**

- libinput, pointer acceleration: https://wayland.freedesktop.org/libinput/doc/latest/pointer-acceleration.html
- libinput, tapping: https://wayland.freedesktop.org/libinput/doc/latest/tapping.html
- libinput, gestures: https://wayland.freedesktop.org/libinput/doc/latest/gestures.html
- libinput, palm detection: https://wayland.freedesktop.org/libinput/doc/latest/palm-detection.html
- Hyprland 0.51 gesture rework: https://hypr.land/news/update51/
- Hyprland per-device keywords via hyprctl: https://github.com/hyprwm/Hyprland/issues/7080
- Chrome, aligned input events and `getCoalescedEvents`: https://developer.chrome.com/blog/aligning-input-events
- W3C Pointer Events (coalesced, predicted, `pointerrawupdate`, `touch-action`): https://w3c.github.io/pointerevents/
- MDN `pointerrawupdate`: https://developer.mozilla.org/en-US/docs/Web/API/Element/pointerrawupdate_event
- MDN `navigator.vibrate`: https://developer.mozilla.org/en-US/docs/Web/API/Navigator/vibrate
- Android `MotionEvent` batching and history: https://developer.android.com/reference/android/view/MotionEvent
- Android `HapticFeedbackConstants`: https://developer.android.com/reference/android/view/HapticFeedbackConstants
- Android gesture navigation and exclusion rects: https://developer.android.com/guide/navigation/gesturenav
- Android Wi-Fi low-latency mode: https://source.android.com/docs/core/connect/wifi-low-latency and https://developer.android.com/reference/android/net/wifi/WifiManager
- Microsoft, Precision Touchpad size: https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-device-integration
- Microsoft, touchpad gestures: https://support.microsoft.com/en-us/windows/touchpad-gestures-for-windows-a9d28305-4818-a5df-4e2b-e5590f850741
- Apple, Multi-Touch gestures: https://support.apple.com/en-us/102482
- GNOME 40: https://forty.gnome.org/ ; https://www.omgubuntu.co.uk/2021/03/gnome-40-new-features
- KDE Plasma 6 gestures (forum summary): https://forum.endeavouros.com/t/kde-plasma-6-wayland-touchpad-gestures-proper-ones/52095
- Deber, Jota, Forlines, Wigdor, *How Much Faster is Fast Enough?* CHI 2015: https://www.tactuallabs.com/papers/howMuchFasterIsFastEnoughCHI15.pdf
- Casiez & Roussel, *No more bricolage!* UIST 2011: https://gery.casiez.net/publications/UIST2011-NoMoreBricolage.pdf
- Finger-pad friction on glass (moisture, stick-slip): https://www.researchgate.net/publication/226856388_Friction_of_the_Human_Finger_Pad_Influence_of_Moisture_Occlusion_and_Velocity ; https://royalsocietypublishing.org/rsif/article/16/156/20190166/87123/Fingerpad-contact-evolution-under

## 11. Open questions — resolved (`2026-10-30`, autonomous run)

1. **Yes, runtime-only.** The daemon may set `device[wado-trackpad]:*` with `hyprctl keyword` (only
   wado's own device, never the laptop's pad, never a config file). The values live in wado's
   `config.kdl` and are re-applied when the device is created. On other compositors the setting
   says plainly that it is not available.
2. **The press bar exists, off by default.** It is offered automatically, with a one-tap switch,
   when the phone sees that taps are not clicking (a tap with no click within the timeout, twice).
3. **No takeover of the user's gestures.** wado does not rebind Hyprland gestures. The phone mirrors
   what the computer does ("Lift: …" for fire-on-lift binds, a 1:1 track for the follow-the-fingers
   ones) and offers the recommended map in §7 as a config snippet to copy. The spec's rule stands:
   never claim a 3-finger swipe.
4. **Not on the phone.** Global gesture distance and flick settings change the laptop's own pad;
   they stay in the user's Hyprland config. The phone shows them read-only, with where to change
   them.
5. **Drag lock off** (the libinput default); "Briefly" is one switch away.
