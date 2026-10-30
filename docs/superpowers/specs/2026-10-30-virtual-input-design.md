# Virtual input: touchpad, keyboard and gamepad as one category

Status: **design, for review**. No product code changed. Prototype: `prototype.html` (open it directly;
the pills bottom-right switch screen, layout, theme and motion; `?state=…&v=…` deep-links every
screenshot, §13). Screenshots: `shots/`.

Binding system: `DESIGN.md` (base16 slots, mixed surface ladder, Figtree / Bricolage / JetBrains Mono,
r1–r4, `--spring` / `--pop`, the duration tokens, the Float-Only Shadow Rule). The prototype copies
`assets/base.css` tokens verbatim; its light scheme is `catppuccin-latte` from `schemes.json`. No hex in
chrome. The face-button colours are base16 slots (A base0B, B base08, X base0D, Y base0A), so they
follow the user's scheme.

Read with: the Computer page spec (`2026-10-30-computer-page-design.md`), the trackpad spec and plan 2,
and the Pocket Console spec (`2026-10-30-pocket-console-design.md`), which is being built now.

---

## 1. The idea in one paragraph

The phone already is the computer's touchpad. It should also be its keyboard and its gamepad, and the
three should read as one thing: **Virtual input**, "this phone as the computer's input". On the
Computer page, one tile replaces the Trackpad door. It opens a hub where each of the three is a live
miniature of itself: tap it to use it, tap its sliders to tune it. Their settings live there, each
page led by a **live preview** that acts out whatever you are changing. The gamepad is rebuilt on a
**layout model**: built-in variants (Xbox-, PlayStation-, Nintendo-style, Minimal, Racing, Fighting,
Retro, and "Same as in a session"), your own saved ones, and an editor where **the gamepad is its own
editor**: drag, resize from a corner, change shape and look, check thumb reach. One model and one
renderer serve the couch (no session, a real Xbox 360 pad on the host) and the game (the overlay or
the Pocket Console deck).

## 2. Decisions in short

| # | Decision | Why |
|---|---|---|
| D1 | A **Virtual input** category with three subcategories, **Touchpad**, **Keyboard**, **Gamepad**. It replaces the Trackpad door, the Trackpad settings row and the in-session "Gamepad" page. | The user's request; one mental model ("my phone as its input") instead of three places. |
| D2 | All three work **with no session**. Touchpad and Keyboard already do (the pad link). Gamepad gains a sessionless path: the pad link carries gamepad reports to the daemon's existing `GamepadInjector` (uinput Xbox 360). | The couch case: Steam Big Picture, emulators and games launched on the desktop read a real controller. |
| D3 | **The Gamepad is gated by the same `input` grant and the same passkey step-up** as the touchpad. No new grant token. | A uinput pad is machine-wide input: it drives Steam's and many desktops' UI navigation, so it is as powerful as the touchpad. A new token would add a column to `trusted_clients` and a checklist item for no real distinction. One grant reads plainly: "this phone may drive this computer's input". In-session play stays as today (session participants already drive input). |
| D4 | The Computer page tile is a **group**: a heading button opens the hub, and three 44 px doors open Touchpad, Keyboard or Gamepad in one tap. | Plain first: the frequent job is "use it", not "configure it". The hub is one tap away. |
| D5 | Hub cards: **tap the card or Open = use it; the sliders button = its settings.** | Same reasoning. Settings are never a dead end: every settings page has Open/Play in its head. |
| D6 | Settings pages lead with a **sticky live preview**: a mini touchpad that slides, scrolls and swipes with a mini screen whose pointer moves by your speed; a keyboard that types and latches; the gamepad itself, pressable. Changing a control re-runs the matching demo. | "Creative and intuitive", and honest: the preview shows what the setting does before the user commits. |
| D7 | **One layout model** for every gamepad surface: sessionless full screen, in-session overlay (cockpit), Pocket Console deck. Away from a session the default is **"Same as in a session"**. | The user asked for "exact controls from the session or any other selected layout". |
| D8 | Positions are **edge-anchored on a reference phone; sizes are CSS px** (physical). | A button is the same size under the thumb on a phone and a tablet, and clusters never spread apart on a big screen (§9.3; screenshots 26 vs 06). |
| D9 | Face buttons bind **by position** (south = A, east = B, west = X, north = Y), whatever the label. The Nintendo-style layout shows B at the bottom and still sends the south button. | The host device is an Xbox 360 pad; SDL and Steam map by position, so games' prompts stay right. |
| D10 | **No new curve, no new duration token.** The gamepad's motion uses `--spring`, `--pop`, `--dur`, `--dur-sheet`, `--dur-hero`, `--dur-press`, `--stagger`, plus DESIGN's two named exceptions (direct manipulation for sticks and drags; ambient loops for the edit jiggle and previews). | The variant morph is a shared-element move, which is what `--dur-hero` already means. |
| D11 | Sessionless **"Keys & mouse"** mode is **later**. v1 sessionless is "A real controller" only, and the page says so. | The daemon's `KeyInjector` can only tap, not hold a key, and there is no relative-mouse device. That is a separate server task (T14). |

## 3. Information architecture

### 3.1 What moves where

| Today | Becomes |
|---|---|
| Computer page big action **Trackpad** (`ui/computer/actions.rs`) | **Virtual input** tile: heading → hub; three doors → Touchpad · Keyboard · Gamepad. Disabled doors say why ("Not allowed for this phone"), as today. |
| Computer page settings row **Trackpad** (opens `pad_settings.js` sheet) | Row **Virtual input** → hub. Subtitle: "Touchpad · Keyboard · Gamepad (Xbox-style)". |
| Trackpad settings sheet (`js/pad_settings.js`), two doors (pad ⚙, Computer row) | Stays one implementation, now the **Touchpad settings page** body (third door: the hub). Inside the pad its ⚙ still opens it as a sheet. The quick-actions editor moves up to the hub ("On all three"), because the list is shared by the rail, the widget and the console's Quick face. |
| In-session control centre page **Gamepad** (`ui/pages/gamepad.rs`: on/off, mode, size, opacity, insets, edit, reset) | **Gamepad settings page** (same component, two hosts: Computer page sub-page and inside the in-session sheet). Size / opacity / insets become per-control and per-layout in the editor; the global sliders go. "On-screen gamepad" on/off stays as the console face / quick-rail button. |
| Home widget **Trackpad** | Home widget **Virtual input**: the touch surface (slide = move the pointer, as today), three doors, the quick-action chips. The last-used door is tinted. |
| Keyboard inside the trackpad screen | Still the same screen. The **Keyboard** door opens it with the keyboard up and the pad as a strip (`keyboard` preference ignored for that open). A standalone keyboard screen is not built. |

### 3.2 The hub (Virtual input page)

Order, top to bottom (screenshot 02, tablet 25):
1. Head with the computer eyebrow (hue dot + name) — the Computer page rule for every sub-page.
2. Lead: "Use this phone as Framework's touchpad, keyboard or gamepad. Works with no session running."
3. **Grant line** (one row, never hidden): Allowed · unlocked / Allowed · locked, "Unlock with your
   passkey" button / Not allowed, with the copyable `wado allow <device> input` command (reuses the
   Computer page's grant card).
4. **Three device cards**, each a live miniature (§5.1): Touchpad, Keyboard, Gamepad (shows the active
   layout's name). Open + sliders button. Three across from 720 px.
5. **On all three**: Feel in the hand (haptics Off/Light/Medium/Strong), Left-handed (mirrors the
   touchpad rail and the gamepad), Quick actions ›.
6. **For every device on Framework** (`host` badge, needs `settings`): Virtual input on Framework ›
   (`trackpad { enabled, smoothing, rate-hz, delay }`). Hidden without the settings grant.

### 3.3 Deep links, shortcuts, Android

| Entry | Today | Now |
|---|---|---|
| `?go=trackpad` (manifest shortcut, `deeplink.rs`) | opens the trackpad on the last computer | **unchanged** (stable URL). Opens the Touchpad. |
| `?go=keyboard` | — | **new**: Touchpad screen with keyboard up. |
| `?go=gamepad` | — | **new**: the Gamepad, full screen, sessionless. |
| `?go=input` | — | **new**: the hub of the last computer. |
| Manifest `shortcuts` (4 today: Trackpad, Files, Shell, Session) | | Keep 4 (Android shows four comfortably): **Touchpad**, **Gamepad**, Files, Start/join. Shell drops to the dynamic shortcut. Open question Q3. |
| Android `shortcuts.xml` (`wado://go/trackpad`) | | Add `wado://go/gamepad`; keep `trackpad`. `links/DeepLinks.kt` maps both to the same `?go=`. |
| Android native trackpad (`ui/pad/PadScreen.kt`) | the whole screen a touchpad; "More" for web extras | It is the **Touchpad** subcategory's native renderer: the Touchpad door opens it in the app, its "More" opens the Touchpad settings page. Settings are the same `device.<key>.trackpad` keys. A native gamepad is **not** in scope; the app shows the web gamepad. |
| `deeplink::decide` | `Go::Trackpad` | Add `Go::Keyboard`, `Go::Gamepad`, `Go::Input`; door permission from `Doors` (§D3: all three need `input`). |

### 3.4 Quick access

- **Computer page**: the tile's three doors (one tap each).
- **Home**: the Virtual input widget (doors + surface + quick chips), screenshot 09.
- **In a session**: the Pocket Console deck faces, and in the cockpit the quick rail's active-face
  button; long-press it lists the faces and a last item "Virtual input settings" (screenshot 09).
- **Control centre in a session**: the two-row list ("This computer ›", "On this phone ›") reaches
  the hub through This computer.

## 4. How it fits the Pocket Console

The console is the **in-session form** of the same three devices; Virtual input is the **sessionless
form** seen from the Computer page. One set of modules, two mounts.

| Pocket Console deck face | Virtual input subcategory | Shared |
|---|---|---|
| Touchpad | Touchpad | `pad_touch`, `pad_pointer`, `pad_gestures`, `pad_feel`; settings `device.<key>.trackpad`. In a session the deck's pad drives the session's input path, sessionless it drives the pad link. |
| Keys | Keyboard | `pad_keys` modifiers and the **key row** (`device.<key>.keyboard.keys`, new). |
| Quick | (hub: Quick actions) | `trackpad { quick }`: one list, now four doors (rail, widget, console face, hub). |
| Gamepad | Gamepad | **The layout model and renderer** (§9). The deck shows the *active layout's* controls, packed by side (below). Editing always happens in the full-screen editor. |

How a layout lands in the console deck: every control has a side (its group centre x < ⅓ left,
> ⅔ right, else centre). Pocket Console T1 (`surround/layout.rs`) gives the deck a left, right and
centre slot; the renderer places each side's groups into its slot, keeping their relative offsets
and per-control size, shape and look, scaled down only as far as the 44 px floor allows. In the
cockpit (full screen) the overlay uses the layout's own positions, exactly as sessionless. So a
change made in the editor shows in all three places, and Pocket Console's S6 ("clusters fly outward
to the overlay corners") flies from the deck slots to these positions.

`device_console.face "gamepad"` stays the console's own choice; which layout it shows is
`device.<key>.gamepad.layout`.

## 5. Screens and components

### 5.1 Hub device cards (`ui/computer/input/hub.rs`)
r3 surface-1 card; a 120 px ground-coloured stage (r2) with the live miniature; row: title (Bricolage
16/700), one-line plain description, 44 px sliders button (surface-2), Open (accent pill, 44 px).
Miniatures: touchpad = pad + mini screen, a finger slides and the pointer follows; keyboard = text
line typing plus the modifier and key rows; gamepad = the active layout rendered at scale, a random
face button presses every 2 s. All three are ambient loops (paused off-screen and under reduced motion,
where they show one still frame).

### 5.2 Touchpad settings (`ui/computer/input/touchpad.rs` hosting `pad_settings.js`) — screenshot 03
- **Preview** (sticky under the head): phone pad drawn the way you hold it, with its rail, beside a
  mini screen. Demos: *slide to move* (pointer travel = finger travel × speed), *two fingers scroll*,
  *tap to click*, *edge swipe → action* (the screen's workspace slides and a toast names the action),
  *hold two → action*. Each control below triggers its demo.
- **How you hold it**: two picture choices, Sideways / Upright (the phone glyph rotates on
  `--spring`), and "Turn the screen for real" (`fullscreen`).
- **Beside the pad**: Keys / Quick actions / Nothing (`rail`), "Open with the keyboard up".
- **Pointer speed** (new, this phone): 0.5–2.0×. Honest caption: "Framework still adds its own
  acceleration."
- **Phone gestures**: four tiles with a tiny picture of the gesture and the action it runs (or Off).
  Tap = demo + the action picker. The note keeps the trackpad spec's promise (wado never claims a
  laptop gesture; all off by default).
- **Decided by Framework**: tap-to-click, scroll direction, acceleration are the host's libinput
  settings; "Where to change them" opens the detail (`hyprctl devices` name).

### 5.3 Keyboard settings (`ui/computer/input/keyboard.rs`) — screenshot 04
- **Preview**: the text line types a sentence, then acts out a shortcut: Super latches (accent, r2),
  "1" goes, a good-tone chip says "Super + 1 → workspace 1".
- **Keys above the keyboard** (new `keys` list): toggle chips (Esc, Tab, arrows, Home, End, PgUp,
  PgDn, Del, Enter, F1–F12). Added keys pop into the preview row (`--pop`). Order = order added;
  a reorder affordance (↑/↓ on long-press) is the single-pointer alternative.
- **Shortcuts**: three tiles show Off / latched / held (the Morphing Toggle Rule: r1 → r2 → pill with
  an inset ring). "Why the next key is a shortcut" is the detail on tap. Switch: "Touchpad above the
  keyboard".
- **Typing**: one info line (autocorrect, swipe, dictation, any language).

### 5.4 Gamepad settings (`ui/computer/input/gamepad.rs`) — screenshot 05
- **Preview**: the active layout, live and pressable; the caption shows what Framework would see.
- **Layout**: a horizontal carousel of variant cards (mini renders). Tapping one morphs the preview
  (§7 G6). Then Edit layout · Comfort check, and **Away from a session, use**: Same as in a session /
  Its own layout (then a second carousel appears).
- **The buttons drive**: A real controller / Keys & mouse, with the plain line and "More about this"
  (`/dev/uinput`, `045e:028e`, made on open and removed on leave; Keys & mouse only in a session
  for now).
- **Sticks**: Fixed / Floating / Relative with a one-line meaning each; Dead zone slider that draws the
  dashed dead-zone ring on the preview's sticks while it moves.

### 5.5 The gamepad screen (`js/gpad/*`) — screenshots 06, 07, 10–13, 22, 26
- Full screen, stage ground (s0 with a faint accent wash; it is not over video, so not stage black).
- **HUD**: two floating pills (Popover shadow: they float). Left: Close, link dot, computer name,
  player chip `P1`. Right: Layouts, Edit, Settings. Landscape: top centre between the shoulders;
  portrait: top edge.
- **Portrait** adds a status card: computer name, "Xbox 360 controller · player 1 · Direct 23 ms",
  **"Framework sees"** chips (live, from what was actually sent) and the stick values in mono. This is
  the honest-status rule applied to a controller: the user can see the press left the phone.
- The HUD's rectangle is a **reserved zone**: the editor will not drop a control under it.

### 5.6 Editor — screenshots 14, 15, 16, 19, 20
- Entering: every control gives one small `--pop` nudge, then a slow ±1.1° **jiggle** (ambient loop,
  phase-shifted per control). A dotted snap grid fades in (40 × 20 cells on the reference phone).
- **Tap** selects a control; by default its **group** (face buttons, a shoulder pair, the middle row)
  is selected with it ("Move together", switchable). Selection: dashed accent outline (r2), a mono
  size badge "1.15× · 74 px", and a resize dot in the corner with a 44 px hit area.
- **Drag** moves (1:1 under the finger), snapping to the grid and to **alignment guides** (screen centre
  and other groups' centres; a 1.5 px accent line shows while aligned). **Corner drag** resizes; a group
  spreads around its centre so its buttons never overlap. The size never goes under 44 px: the drag
  and the slider stop there.
- **Edit bar** (top, floating): Undo, Redo, layout name ("· edited"), Comfort check, Mirror (left
  hand), Snap, **Done**.
- **Inspector** (landscape: a floating card docked on the side *away* from the selection; portrait: a
  bottom sheet, and the pad lifts so the selection stays above it). Three tabs:
  - **Place**: Size slider, See-through slider, **nudge arrows (8 px)** — the single-pointer alternative
    to dragging (WCAG 2.5.7) — Move together switch, Hide, Reset this control.
  - **Look**: scope This one / All controls; Shape (Circle, Pill, Rounded); Style (Solid, Outline,
    Flat); Labels (Letters, Shapes ✕○□△, Colours); On press (Glow, Ring, Still). Each choice is a
    picture of itself.
  - **Behave**: per kind — stick: Fixed / Floating / Relative + dead zone (ring drawn live); d-pad:
    4-way cross / 8-way; steering: Sliding / Tilting the phone; button: what it sends (Xbox button in
    controller mode, any key in Keys & mouse); and Feel in the hand.
  - With nothing selected: Reset, Layouts, Save as a new layout, On press for all, and the note that
    sizes are kept in real pixels.
- **Done** saves to the computer (per device). Leaving the editor with unsaved changes never asks:
  every change is saved as it is made; Undo is the safety net (60 steps, kept until the screen closes).

### 5.7 Variant picker — screenshots 08, 17
A sheet (portrait bottom, landscape right side, so the pad stays visible). "Tap one to try it. It
changes behind this sheet." Built in (with "Same as in a session" first and a `session` badge), then
Yours, with "Copy this one". Mini renders are the real renderer at scale.

### 5.8 Comfort check — screenshots 18, 27
Two thumb-reach fields drawn from the hand pivots (landscape: the bottom corners; portrait: low on
each side): green = easy for a resting thumb (45 mm), yellow = a stretch (62 mm), converted to px by
a per-form estimate (phone 5.6 px/mm, tablet 4.7) and a Small / Average / Large hands switch. Flags
(warn tone, breathing ring on the control, and a plain list item that selects it):
"A is a stretch for your right thumb", "LT is out of reach…", "X is smaller than 44 px",
"LB and RB are too close to tell apart" (< 8 px). The middle row is never flagged for reach (pressed
rarely, on purpose). **Pull into reach** moves flagged controls along the line to their pivot until
they sit inside green, with the variant morph. The card says it is an estimate, never a measurement.

### 5.9 Copy (plain first)
- Category: **Virtual input**. Doors: **Touchpad**, **Keyboard**, **Gamepad**.
- Gamepad modes: **A real controller** / **Keys & mouse**.
- Errors, from `InputError::Unavailable` and the gate: "Framework can't make a controller: wado needs
  to be allowed to use `/dev/uinput`. On Framework: `sudo usermod -aG uinput $USER`, then log in again."
  / "This phone may not drive Framework's input — on the computer: `wado allow pixel-8 input`".
- Older daemon: "Framework's wado is too old for the gamepad away from a session. Update it, or use the
  gamepad inside a session."

## 6. Gamepad variants

All send Xbox 360 codes (`045e:028e`), by position. Landscape and portrait positions both ship.

| Variant | Controls | Default look | Notes |
|---|---|---|---|
| **Same as in a session** | the in-session pad's controls (`gamepad.js` CONTROLS), with its edits | as edited | Default for sessionless play (D7). |
| **Xbox-style** | LT LB RB RT, View · Guide · Menu, left stick high, d-pad low, right stick low, A B X Y | Colours (A base0B, B base08, X base0D, Y base0A) | Default in a session. |
| **PlayStation-style** | L2 L1 R1 R2, d-pad high, sticks low and inward, ✕ ○ □ △ | Shapes (✕ base0D, ○ base08, □ base0E, △ base0C) | Glyphs by position. |
| **Nintendo-style** | ZL L R ZR, − ⌂ +, left stick high, d-pad low, B below / A right / Y left / X top | Letters, monochrome | Labels swap; codes stay by position (D9). |
| **Minimal** | D-pad (1.15×), B (1.2×), A (1.35×), Start | Large targets | Menus, casual and emulator UIs. |
| **Racing** | Steering slider (or tilt), Brake and Gas **analog pedals** (slide up = more), A X, LB RB, Start | Rounded | Pedals send LT/RT 0–255 by finger height. |
| **Fighting** | 8-way d-pad (1.1×), six buttons in two arcade rows (X Y RB / A B RT), View, Menu | Letters | 8-way on by default. |
| **Retro** | D-pad, B A (1.15×, diagonal), Select Start (words) | Letters | Portrait first (a handheld). |
| **Yours** | any | any | Saved from any variant; "from Racing" shown under the name. |

Base sizes (CSS px at 1×): face button 64, centre button 64×44, bumper 84×46, trigger 84×50, d-pad
150, stick 128, pedal 96×170, steering 230×76. **Every default is ≥ 56 px on its short side except the
44 px centre row**; nothing can be made smaller than 44 px.

## 7. Motion

Personality: **tactile and quick on the pad, calm in the chrome**. Only `--spring` (spatial moves) and
`--pop` (things a finger just flipped). transform / opacity / `translate` / `scale` / `rotate` only.

| # | Trigger | What moves | Timing | Reduced / off |
|---|---|---|---|---|
| G1 | Gamepad opens | **Assemble**: groups slide in from their nearest edge (≈ 35 px × distance from centre) and scale .82 → 1 with opacity, in thumb order: left stick, d-pad, steering, right stick, face, pedals, shoulders, middle. Face buttons then pop .6 → 1, 20 ms apart. | `--dur-sheet` 480 ms `--spring`, `--stagger` 40 ms between groups (total ≤ 320 ms); pops `--dur`+80 `--pop` | 160 ms fade / none |
| G2 | Press a button | Face `scale` .92 and fills toward its colour; a halo ring blooms .92 → 1.35 and fades (Glow) or holds (Ring) or nothing (Still); haptic tick by strength (6 / 10 / 18 ms) | press `--dur-press` 120 ms `--spring`; halo 360 ms `--spring`; release `--dur` `--pop` | press and colour stay (feedback ≤ 120 ms); no halo |
| G3 | Stick | Thumb follows the finger **1:1** (direct manipulation); a direction nub on the rim rotates to the angle with opacity = deflection; three **trail** dots echo the last positions (sampled history, no easing); below the dead zone the thumb dims | 1:1; release: thumb springs home `--dur` 220 ms `--pop` (the one place a stick overshoots) | no trail; release snaps |
| G4 | Floating stick | The base jumps to the thumb, then goes home on release | release `--dur-sheet` `--spring` | snaps |
| G5 | D-pad | Arms light and scale .94; a direction change ticks once | `--dur-press` | same |
| G6 | Change variant | **Morph** (FLIP): controls with the same id fly and resize from old to new rect (a bumper becomes a round button by non-uniform scale); labels cross-fade (.35 → 1, 160 ms); leaving controls shrink to .6 and fade; new ones pop in after 200 ms. Interruptible: a second tap snapshots the visual rects and flips from there. | `--dur-hero` 520 ms `--spring`, groups `--stagger`×0.6 apart; leave 180 ms accelerate (DESIGN's named exit); arrive 300 ms `--pop` | 160 ms fade / instant |
| G7 | Enter edit | One `--pop` nudge per control (scale 1.06), grid fades in, then the jiggle loop (±1.1°, 1.1 s alternate, phase per control). The selected control stops jiggling. | nudge `--dur`+100 `--pop`, 20 ms apart; jiggle is an ambient loop (`ease-in-out`) | no nudge, no jiggle (dashed outline only) |
| G8 | Select | Selection box pops .94 → 1; resize dot appears with it | `--dur` `--pop` | instant |
| G9 | Drag / resize | 1:1, grid and guides appear; on release a light haptic | none (direct manipulation) | same |
| G10 | Look change | Affected faces pop .7 → 1 with opacity .4 → 1 | 300 ms `--pop` | instant |
| G11 | Undo / Redo / Mirror / Pull into reach | The morph G6, shorter for undo | undo `--dur`+120 `--spring`; others `--dur-hero` | instant |
| G12 | Portrait inspector | The pad lifts so the selection clears the sheet | `--dur-sheet` `--spring` | instant |
| G13 | Comfort check | Reach fields grow from their pivots (.6 → 1); flagged controls get a breathing warn ring | `--dur-sheet` `--spring`; breathe 2.4 s ambient | static |
| G14 | Picker sheet | Slides in (bottom / right) | `--dur-sheet` `--spring` | 200 ms |
| G15 | Settings previews | Demo loops (finger, pointer, typing, gesture toast) | 1.4 s per demo, re-run every 2.4 s; segments `--spring` | one still frame |
| G16 | Hub sections | Rise 18 px + fade, `--stagger` apart (Computer page M3) | `--dur-hero` `--spring` | none |

Three layers on the pad: primary (the press or the stick), secondary (halo, trail, direction nub,
haptic), ambient (the wash; the jiggle in edit). Motion never sits on the send path: every press is
sent in `pointerdown` before any animation is started.

## 8. Accessibility (WCAG 2.2 AA)

- **Targets**: ≥ 44 px always (enforced by the model and the editor), ≥ 8 px apart (comfort check
  warns), defaults 56–64 px. Inspector controls 44–52 px.
- **Dragging (2.5.7)**: every drag has a single-pointer path: nudge arrows, the Size slider, ↑/↓
  arrows on the keyboard picker, Mirror and Pull into reach. Keyboard: arrows nudge the selection,
  Ctrl/Cmd+Z / Shift+Z undo and redo.
- **Names**: each control is a `button` named by its function ("A button", "Left stick", "D-pad");
  sticks and pedals are `role="slider"` with `aria-valuetext` ("up-right, 70%"). The HUD and edit bar
  are `role="toolbar"`; the play area `role="application"` named "Gamepad for Framework". The
  inspector is a labelled region; the comfort check list is a list of buttons.
- **Live regions**: one polite region for "Saved as …", "Mirrored for the left hand", variant changes
  ("Layout: Fighting"). Presses are not announced (they would flood).
- **Colour**: face colours always come with a letter or a shape; "Colours" labels keep the letter.
  Status chips sit on s1. Verify contrast for base0A/base0B labels on s2 in light schemes (latte's
  base0A on a light s2 is the weakest pair; labels are ≥ 22 px bold = large text, needs 3:1).
- **Motion**: `prefers-reduced-motion` and `data-motion` (full / reduced / off) honoured per §7.
- **Haptics** are an addition, never the only feedback; Off is a choice.

## 9. Layout model (for engineers)

### 9.1 The shape

```jsonc
// wado_protocol::gamepad::Layout — JSON on the wire and in the client mirror
{
  "v": 1,
  "id": "mine-3f2a",             // built-ins: "xbox", "ps", "nintendo", "minimal", "racing", "fighting", "retro", "session"
  "name": "Couch racer",
  "base": "racing",               // the built-in it was made from; overrides apply on top of it
  "look": { "shape": "round", "style": "outline", "labels": "letters", "press": "glow", "op": 0.92 },
  "stick": "floating",           // fixed | floating | relative
  "dead": 0.12,                   // 0..0.4 of the stick radius
  "controls": {                   // only what differs from `base`
    "rt":      { "l": [0.90, 0.62, 1.10] },                 // [x, y, size] landscape
    "a":       { "p": [0.84, 0.56, 1.0], "look": { "shape": "circle" } },
    "dpad":    { "hidden": true },
    "steer":   { "steer": "tilt" },
    "x":       { "keys": { "key": 18 } }                    // Keys & mouse binding (in a session)
  }
}
```

A **built-in variant** is the full form, shipped in the client (`js/gpad/variants.js`), never stored:

```jsonc
{ "id": "xbox", "name": "Xbox-style", "look": {…},
  "controls": [ { "id": "a", "kind": "btn", "bind": "a", "label": "A", "g": "face",
                  "l": [0.86, 0.78, 1], "p": [0.73, 0.715, 1] }, … ] }
```

- `kind`: `btn` | `small` | `bumper` | `trig` | `dpad` | `stick` | `slider` | `pedal`.
- `bind`: what it sends as a controller: `a b x y lb rb back start guide l3 r3` (`BTN_*`), `lt rt`
  (`ABS_Z` / `ABS_RZ`, 0–255; a pedal is analog), `hat` (`ABS_HAT0X/Y`), `ls rs` (`ABS_X/Y`,
  `ABS_RX/RY`; a `slider` drives only X).
- `g`: the move-together group. `dir`: 4 | 8 for a d-pad.
- `l` / `p`: `[x, y, size]`. x, y are the centre as fractions of the **reference play area** (the safe
  area of 844×390 landscape, 390×844 portrait, minus 10 px); size multiplies the kind's base px.
- Keys & mouse bindings default to today's `KEYMAP` / `WASD` / mouse-look and are overridden per control.

### 9.2 Storage: per device, on the computer

Like `device_trackpad.rs`: typed, readable in `config.kdl`, settable by the device itself without the
settings grant (`config::link` self-device rule). The client keeps a localStorage mirror so the pad
opens laid out before the daemon answers.

```kdl
device "pixel-8" {
    keyboard { keys "esc" "tab" "left" "up" "down" "right" }
    gamepad {
        drives "pad"              // pad | keys
        layout "xbox"             // the layout in a session (and the console deck)
        away "same"               // "same", or a layout id for sessionless play
        haptics "medium"          // off | light | medium | strong   (shared by all three)
        hand "right"              // right | left                     (shared by all three)
        mine "mine-3f2a" name="Couch racer" base="racing" json="{…controls and look…}"
    }
}
```

- `mine` carries the override object as compact JSON: a layout is written by the editor, not by hand,
  and typing every control into KDL would make each new control kind a schema change (the same
  reasoning as `device { prefs }`). Everything a person might hand-edit (`drives`, `layout`, `away`,
  `haptics`, `hand`, `keys`) is typed. At most **8** saved layouts per device; each `json` ≤ 4 KB.
- `haptics` and `hand` live in `gamepad` for v1 but mean "all three"; if that reads wrong, they move to
  a `device { input { } }` block (Q4).
- `trackpad` gains `speed` (0.5–2.0, default 1.0): the daemon divides `px_per_mm` by it when the pad
  reports its size, so libinput sees a bigger or smaller pad (that is what moves the pointer further).
- Migration: on first load, the old `wado.padLayout` localStorage overrides become a saved layout
  "My layout (from before)" based on "session", and `pad_mode` → `drives`. `pad_scale`,
  `pad_opacity`, `pad_inset_*` fold into that layout's sizes and look, then are dropped.

### 9.3 Placing a layout on a screen

`placePx(control, layout, orientation, k, box)` (prototype: `src/app.js`):
1. Group centre = the mean of its members' `[x, y]`.
2. Anchor the group centre by thirds of the reference area: x < ⅓ → keep `x·W_ref` px from the left
   edge; x > ⅔ → keep `(1−x)·W_ref` px from the right edge; else keep `(x−½)·W_ref` px from the centre.
   Same for y. Multiply by `k` (1 on the play screen; box/W_ref for thumbnails and the console deck).
3. Members keep their px offset from the group centre. Size = base × size × k.
So a phone layout opened on a tablet keeps the clusters together at the edges and under the thumbs,
and the extra space goes to the middle (screenshot 26). Dragging edits fractions in reference units
(1 px of finger = 1/W_ref).

## 10. Engineering notes

### 10.1 Client only
Hub, settings pages, previews, widget, tile, editor, picker, comfort check, variants, renderer, morph,
migration, `?go=` additions, Android shortcut and deep-link mapping. The in-session gamepad keeps
sending `gamepad_button` / `gamepad_axis` over the session input channel (`W.coalesce`), unchanged.

### 10.2 Needs the server
1. **Sessionless gamepad over the pad link** (`crates/protocol/src/pad.rs`, additive variants, tag `k`):
   - `GameOn` / `GameOff` (keys channel, reliable): open / close the controller. The daemon opens a
     device from the registry's `gamepad()` opener (`wado_platform::input::GamepadInjector`, already
     offered by `server/platform/linux` via `wado_compositor::input::gamepad::opener`), on first
     `GameOn`, and destroys it on `GameOff`, `Bye`, link drop, or 30 s of silence. Games enumerate
     controllers at start, so the device stays for the whole time the Gamepad screen is open,
     not per press.
   - `Btn { code, down }` (keys channel, reliable, ordered): buttons, hat and digital triggers.
   - `Axes { seq, a: [lx, ly, rx, ry, lt, rt] }` (touch channel, unreliable, newest-wins): **the whole
     axis state** in one frame, re-sent every 100 ms while any axis is off-centre. The daemon zeroes
     the axes after 250 ms of silence, so a lost "back to centre" can never leave a stick held (the
     same rule the touchpad uses for fingers).
   - Relayed, they ride `RelayMsg::PadIn` unread (relay unchanged). An older daemon would reject the
     unknown variants, so the daemon announces `pad-gamepad` in its caps and the client hides the
     sessionless Open on older daemons with the plain line in §5.9.
2. **Gate** (`server/pad/gate.rs`): `refused` + `step_up` for every `GameOn`, `Btn` and `Axes`, exactly
   as for touch; `trackpad { enabled }` off refuses it too (it is the host's "virtual input" switch; a
   rename to `input { }` with an alias is Q5).
3. **Sink** (`server/pad/sink.rs`): a third device beside touchpad and keyboard, made on first use.
4. **Config**: `device_gamepad.rs`, `device_keyboard.rs`, `trackpad.speed`; `ConfigState` /
   `ConfigSet` fields; the self-device write rule; tests mirroring `device_trackpad.rs`.
5. **Later (T14)**: sessionless Keys & mouse needs `KeyInjector::press(code, down)` and a relative
   pointer device (uinput `REL_X/Y`, `BTN_LEFT/RIGHT`).

Invariant 1 holds: gamepad traffic rides the pad link's own channels (its own peer connection), never
behind video; axes are newest-wins. Relayed play inherits the trackpad's logged exception
(Decision Log 2026-10-29) and the link pill says "Relayed".

**Measure before claiming**: report press-to-`evtest` latency on LAN and relayed (phone timestamp →
daemon log), next to the trackpad's numbers. No latency claim in UI copy.

## 11. Build plan (independent tasks, one module each)

| Task | Module(s) | Depends on |
|---|---|---|
| **T1** Layout model + variants | `js/gpad/variants.js` (data), `js/gpad/model.js` (resolve base + overrides, clamp ≥ 44 px, mirror, diff for save); unit test with `node` | — |
| **T2** Placement | `js/gpad/place.js` (§9.3 anchoring, thumbnails `k`, console-deck packing by side) + tests | T1 |
| **T3** Renderer | `js/gpad/render.js` (element per id, kinds, looks, labels) + `assets/gpad.css` | T1, T2 |
| **T4** Input on the pad | `js/gpad/input.js` (buttons, d-pad 4/8, sticks fixed/floating/relative, dead zone, pedals, steer, tilt) emitting one abstract event; `js/gpad/send_session.js` (today's coalesce wire); readout | T3 |
| **T5** Motion | `js/gpad/motion.js` (assemble, press halo, trails, morph FLIP, edit nudge/jiggle, reduced/off) | T3 |
| **T6** Editor | `js/gpad/edit.js` (select, group, drag, snap, guides, resize, nudge, undo/redo) + `js/gpad/inspector.js` | T3, T5 |
| **T7** Picker + comfort | `js/gpad/picker.js`, `js/gpad/comfort.js` | T3, T5 |
| **T8** Config schema | `config/schema/device_gamepad.rs`, `device_keyboard.rs`, `trackpad.speed`; `ConfigState`/`ConfigSet`; self-device rule; tests | — |
| **T9** Prefs bridge + migration | `js/gpad/prefs.js` (mirror + ConfigSet, like `pad_prefs.js`), migration of `wado.padLayout` / `pad_*` | T1, T8 |
| **T10** Wire | `protocol/src/pad.rs` `GameOn/GameOff/Btn/Axes` + round-trip tests; caps `pad-gamepad` | — |
| **T11** Daemon sink | `server/pad/game.rs` (device lifecycle, axis silence), gate checks in `live.rs`, sink entry; tests with the fake platform | T10 |
| **T12** Sessionless send | `js/gpad/send_pad.js` (pad link: `Btn` reliable, `Axes` snapshot unreliable + keep-alive) | T4, T10 |
| **T13** IA: hub + pages | `ui/computer/input/{mod, hub, touchpad, keyboard, gamepad, grant_line}.rs`; tile in `actions.rs`; row in `settings_list.rs`; remove `ui/pages/gamepad.rs`; in-session sheet hosts the same pages | T9 |
| **T14** Previews | `js/input_preview/{touchpad, keyboard}.js` (demo loops driven by the settings) | T13 |
| **T15** Entry points | `deeplink.rs` (`Go::Keyboard/Gamepad/Input`), manifest shortcuts, Android `shortcuts.xml` + `DeepLinks.kt`, home widget re-cut, quick-rail long-press list | T13 |
| **T16** Console hook-up | Pocket Console T5's gamepad face reads the layout via T2's deck packing | T2, PC T5 |
| **T17** Pointer speed | daemon divides `px_per_mm` by `trackpad.speed` on `Size` | T8 |
| **T18** (later) Sessionless Keys & mouse | `KeyInjector::press`, relative pointer device, `send_pad.js` keys path | T11 |
| **T19** Verify | screenshots (390×844, 844×390, 820×1180; default-dark + latte); reduced-motion and keyboard passes; contrast on three schemes; `evtest` on the host shows the pad; no stuck button or axis after killing the browser mid-press; latency numbers | all |

Order: T1, T8, T10 first (independent); then T2–T5, T9, T11 in parallel; then T6, T7, T12, T13;
then T14–T17; T19 last. T18 when wanted.

## 12. Decisions taken (for review) and open questions

Taken: D1–D11 above, plus:
- The hub's sliders button and the Open button are separate targets; the card's preview is an Open.
- The editor saves continuously; no "Discard changes?" dialog.
- Shoulders and the middle row are excluded from "a stretch" warnings; only out-of-reach (beyond 1.3×
  the stretch radius) is flagged for shoulders, never for the middle row.
- Variant names use brand words as descriptions ("Xbox-style"), never logos or brand glyph fonts;
  the shapes ✕ ○ □ △ are drawn as plain SVG.

Open:
1. **Brand words** in variant names: fine as descriptive, or rename to "Classic", "Shapes", "Swapped
   A/B"? (I would keep "-style"; they are what people search for.)
2. **Tablets**: offer "Bring controls closer?" once, when the comfort check flags a built-in layout on
   first open (screenshot 27 shows why), or ship tablet positions per variant?
3. **Shortcut slots**: Touchpad, Gamepad, Files, Start/join (Shell becomes dynamic), or keep Shell?
4. `haptics` / `hand` under `gamepad { }` (v1) or a `device { input { } }` block now?
5. Rename the host's `trackpad { enabled }` to `input { enabled }` (with an alias) now that it also
   gates the gamepad?
6. **Device lifetime**: 30 s grace after leaving the Gamepad screen, so a game does not lose its
   controller while the user peeks at the touchpad? (Proposed: yes.)
7. **Player number**: when a session's in-compositor pad and a sessionless pad both exist on one host,
   they are two controllers (P1, P2). Show the real index from the daemon, or always "P1"?

## 13. Prototype: what is real and what is not

`prototype.html` is self-contained (Google Fonts only). Built from `src/head.html` + `src/app.js`
by `build.sh`; screenshots by `node shoot.mjs` (headless Chrome over CDP; it kills only the Chrome it
started).

**Interactive**: switching screens (tile, hub, settings, gamepad); gamepad presses, d-pad 4/8-way,
sticks (fixed / floating / relative, dead zone, trails), pedals with analog fill, steering; the
"Framework sees" readout; variant picker with the live morph; the editor (select, group, drag with
grid snap and guides, corner resize with the 44 px floor, nudge, size and see-through sliders, all Look
options with "these / all", stick and d-pad behaviour, hide, reset, undo/redo, mirror, save as);
comfort check (zones, flags, hand size, select from list, Pull into reach); settings previews
(hold, rail, speed, gesture demos; keyboard key picker and typing demo); haptics through
`navigator.vibrate` where the browser has it; light/dark and motion full/reduced/off.

**Not real**: nothing reaches a computer; the touchpad and keyboard screens themselves are not drawn
(their Open shows a toast: they already exist); the grant line, link state and passkey are fixed
text; tilt steering is a setting with no sensor; Keys & mouse bindings are not editable; saved layouts
vanish on reload; the console deck form (§4) is described, not drawn; the hub's host row, quick
actions row and toggles do nothing.

### Screenshots (`shots/`, @2x)

| File | Viewport | State |
|---|---|---|
| `01-computer-page` | 390×844 | Computer page with the Virtual input tile and row |
| `02-hub` | 390×844 | The hub |
| `03-touchpad-settings` | 390×844 | Touchpad settings, preview mid edge-swipe demo |
| `04-keyboard-settings` | 390×844 | Keyboard settings, preview after typing |
| `05-gamepad-settings` | 390×844 | Gamepad settings: live pad, layout carousel |
| `06-play-portrait-xbox`, `07-play-portrait-retro` | 390×844 | Gamepad, portrait, with the "Framework sees" readout |
| `08-picker-portrait` | 390×844 | Layout picker sheet |
| `09-widget-and-quick-rail` | 390×844 | Home widget and the in-session quick-rail face list |
| `10-play-xbox`, `11-play-playstation`, `12-play-racing`, `13-play-fighting` | 844×390 | Variants in play (10: A and RB down, left stick deflected) |
| `14-edit-look`, `15-edit-place`, `16-edit-behave-deadzone` | 844×390 | Editor with the inspector's three tabs |
| `17-picker-landscape` | 844×390 | Picker as a side sheet |
| `18-comfort-check` | 844×390 | Comfort check on PlayStation-style |
| `19-edit-portrait` | 390×844 | Portrait editor: the pad lifts above the sheet |
| `20-light-edit`, `21-light-hub`, `22-light-play-portrait` | | catppuccin-latte |
| `23-mid-assemble-t180` | 844×390 | Assemble frozen at 180 ms |
| `24-mid-morph-xbox-to-fighting-t110` | 844×390 | Variant morph frozen at 110 ms |
| `25-tablet-hub`, `26-tablet-play`, `27-tablet-comfort` | 820×1180 | Tablet: three-across hub, edge-anchored pad, comfort flags |
| `28-reduced-motion-play` | 844×390 | Minimal, reduced motion |

## Open questions — resolved (`2026-10-30`, autonomous run)

1. **Variant names carry no brand words.** "Standard (A B X Y)", "Shapes (△ ○ ✕ □)", "Swapped
   (B A Y X)", plus Minimal, Racing, Fighting, Retro. The glyphs say which family each one is.
   Bindings stay positional (Xbox codes), as decided above.
2. **Tablets:** a one-time "Bring controls closer?" prompt that runs Pull into reach. No
   per-variant tablet positions: one model, and the comfort fix already exists.
3. **Manifest shortcuts stay** Trackpad / Files / Shell / Desktop. The Trackpad shortcut lands in
   Virtual input → Touchpad. Gamepad is a widget and a quick-rail entry, not a launcher shortcut.
4. **`haptics` and `hand` live under `gamepad {}`.** No new `device { input {} }` block until
   a second module needs them.
5. **`trackpad { enabled }` keeps its name** and gates all three. Documented as such; renaming
   a config key users already have costs more than it buys.
6. **The uinput gamepad is kept for 30 s** after leaving the Gamepad screen, so flipping away
   and back does not make games see a disconnect and reconnect.
7. **Player numbers are the kernel's.** A session pad and a sessionless pad are separate devices,
   and games number them as they appear. Said in the hub when both exist.

Mid-animation and light-theme shots beyond those committed live in the prototype
(`virtual-input/prototype.html?state=…`).
