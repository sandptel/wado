# Pocket Console: design spec for the session view

Status: **design, decided autonomously**. No product code has changed, and every call below is
logged in §11 for review. The prototype is `prototype.html`. Open it directly; the pills at the
top switch full screen, people, software encode, theme and motion. `?state=…` deep-links every
screenshot (§10).

Binding system: `DESIGN.md` (base16 slots, the mixed surface ladder, Figtree / Bricolage /
JetBrains Mono, r1–r4, `--spring` / `--pop`, the Float-Only Shadow Rule). The prototype copies
`assets/base.css` tokens verbatim, and its light scheme is `catppuccin-latte`. Chrome contains no
hex. The only literal colours are inside the fake desktop in the picture, which is content.

Reused vocabulary: the dock (`ui/dock.rs`), the workspace bar (`ui/workspaces.rs`), the quick rail,
the link badge (`ui/link_badge.rs`), the trackpad's keys and quick actions (`js/pad_rail.js`,
`pad.css .tpkey/.tpact/.tptype`), the trackpad surface (`js/pad_touch.js`, `pad_pointer.js`,
`pad_gestures.js`), the on-screen gamepad's clusters (`js/gamepad.js`) and the window sheet
(`workspaces::sheet`). F6 inputs come from `docs/superpowers/specs/2026-10-30-multi-client-design.md`
(presence channel, participants list, lead).

---

## 1. The idea in one paragraph

A session opens as a **console**: the picture sits inset at its true aspect inside a shell, and the
controls sit *around* it. The window controls form a chin under the picture. Workspaces run down one
side and input down the other. A **deck** gives the thumbs a touchpad, keys, quick actions or a
gamepad. **Full screen** grows the picture to the whole display, and the *same elements* travel
outward into today's Glass Cockpit: the dock, the quick rail, the link badge, plus the new people
chip. Nothing is re-rendered. Every control is one DOM node with two positions, so focus, screen
reader position and in-flight presses survive the morph.

## 2. Principles that decided the layout

1. **The picture is never re-laid out. The console is a transform.** `#wado-video` always keeps the
   session's own size. Console mode applies one `translate + scale` to it, and full screen sets that
   transform to identity. No new `Output`, no IDR, no app reflow, no stretched frame mid-morph.
   (Rejected: sizing the session to the console. Invariant 8 would demand a fresh Output on every
   toggle, apps would reflow, and the morph would show a stale frame for the length of an IDR.
   It is parked as an open question, §12.)
2. **The gutter law.** The output has the display's own aspect. So a band on one axis always leaves
   proportional gutters on the other: bands above and below a portrait picture leave about 50 px
   on each side. We decide what fills both axes instead of leaving dead bars. Each form factor
   therefore has a *band pair* and a *rail pair*.
3. **Thumbs decide where things go.** On touch, the surround sits where thumbs rest: at the bottom
   in portrait, at the sides in landscape. With a mouse, it frames the picture evenly and nothing
   chases reach.
4. **Where on the left, how on the right.** The left rail holds where you are (workspaces and
   windows). The right rail holds how you drive it (rotate, keyboard, input face). Landscape keeps
   that split as two columns, and the trackpad's `flip` preference mirrors it for left-handed use.
5. **Buttons fly, plates cross-fade.** A control is a button that moves. The surfaces behind controls
   (the console bezel, the deck, the cockpit's pill groups) do not move. They fade out where they
   were and fade in where they will be. This keeps text and icons undistorted, keeps every animated
   property compositor-only, and makes the Float-Only Shadow Rule visible: the shadows arrive with
   the cockpit plates, at the moment things start floating over the picture.
6. **The dock row is the anchor.** In portrait, Apps · Home · Back · ⋯ stay where they are through
   the morph, and only their plate changes. The eye always has something still to hold on to.
7. **The console may say more, and the cockpit compresses.** The console has room for words: "Direct · 23 ms",
   "Software encoding", the computer's name, the head-count. In the cockpit the same elements fold to
   the dot, the LED and the stacked avatars. Honest status stays honest in both (invariant 5).

## 3. Information architecture: one set of controls, two arrangements

| Control (reused unless marked) | Console: portrait phone | Console: landscape phone | Console: desktop | Cockpit (all) |
|---|---|---|---|---|
| **Link** (`link_badge.rs`) | Top band left, words: "Direct · 23 ms" | Top band left, words | Top band left, words | Top-left, collapses to the dot when direct (as today) |
| **Software encoding** (invariant 5) | Top band chip "Software encoding" (replaces the name) | Top band chip | Top band chip | Folds into the ⋯ LED (orange, breathing; as today) |
| **Computer name** *(new placement)* | Top band, hue dot + Bricolage 15/700 | Top band | Top band | Folds into the link badge (fades) |
| **People chip** *(new, F6)* | Top band right: avatars + count | Top band right | Top band right | Top-right: avatars only |
| **Workspaces** (`workspaces::bar`) | Left rail, vertical: numbers, the active one's app icons, `+` | Top band, horizontal, centred | Left rail, vertical | Dock left group (own row when ≤430 px, as today) |
| **Window strip** *(new)*: focused app, Minimise, Maximise/Restore, Close | Chin under the picture, full bezel width | Band under the picture | Bottom band, left | Folds into the focused app's icon on the dock; long-press it for the window sheet (as today) |
| **Apps · Home · Back** (`dock.rs` mid) | Dock row, bottom centre | Left column, 2×2 with ⋯ | Bottom band centre | Dock mid group |
| **⋯ Control centre + LED** | Dock row right | Left column | Bottom band right | Dock right group |
| **Full screen** (the toggle) | Dock row left, labelled "Full screen" | Left column foot, labelled | Top band right, labelled | Quick rail, icon `min` ("Leave full screen") |
| **Rotate · Keyboard** (quick rail) | Right rail top | Top band right | not shown (mouse and keyboard) | Quick rail |
| **Input faces** *(new)*: Touchpad · Keys · Quick · Gamepad | Right rail foot | Left column, 2×2 | Bottom band right (no Touchpad) | Quick rail shows only the active face; the others fold into it |
| **Deck** *(new container)* | Under the chin, takes all leftover height | Right column, full height | Right column beside the picture | Sinks under the dock (stowed) |
| **Gamepad clusters** (`gamepad.js`) | In the deck (chin hidden) | Both columns become the controller | Right column | Fly outward to the overlay corners (today's overlay) |
| **Other participants' pointers** *(new, F6)* | On the picture | On the picture | On the picture | On the picture |

Quick **tiles** stay in the control centre. The deck's Quick face holds quick **actions** instead:
host actions run often in a session, while tiles are client toggles touched rarely. The list is the
existing `trackpad { quick }`, which already drives the trackpad rail and the home Trackpad widget.
**One list, three doors.**

## 4. Layout per form factor

Measured from the prototype's layout function. The gutters are 6 px on phones and 10 px on desktop,
with safe-area insets on every edge.

| Form | Detection | Bands | Rails / columns | Picture scale |
|---|---|---|---|---|
| Portrait phone (390×844) | short side < 600, portrait | top 40 (status) · chin 44 · deck ≥ 88 (gets the leftover) · dock row 52 | 48 + 48 | **≈ 69%** (270×584) |
| Landscape phone (844×390) | short side < 600, landscape | top 44 (status, workspaces, rotate, keyboard) · under-picture band 40 (window strip) | nav column 104 · deck column 104 | **≈ 72%** (610×282) |
| Portrait tablet (820×1180) | short side ≥ 600, coarse pointer | top 48 · chin 44 · deck ≥ 150 · dock 56 | 56 + 56 | **≈ 72%** |
| Landscape tablet | short side ≥ 600, coarse, landscape | as landscape phone | columns 140 | ≈ 74% |
| Desktop (1280×800) | fine pointer | top 44 · bottom band 48 (window strip, nav, faces, ⋯) | workspace rail 56 · deck column 96 | **≈ 83%** (1062×664) |

- **Leftover goes to the deck.** When the picture is bound by width, the remaining height goes to the
  deck, so a taller phone gets a bigger touchpad and never a bigger dead band.
- **Joiners (F6) letterbox for free.** A participant's picture has the lead's aspect, and the same fit
  places it. In the console the deck absorbs the difference. In the cockpit it is letterboxed on stage
  black, as F6 specifies.
- **Software-encode, narrow phones.** In the portrait top band the encoder chip takes the name's
  slot. The status is never dropped for lack of room.
- **The face switcher on desktop has no Touchpad**, because a real mouse makes it pointless. Keys
  stays: Super, Alt+Tab and similar combinations are what a browser swallows, and the latching
  modifier keys get them to the computer.

## 5. Components

### 5.1 Inventory

| Component | Status | Notes |
|---|---|---|
| `#wado-video` + `#stage-video` | **reused, changed** | Always the session's own size; gains a transform from `surround::layout`. Input mapping must read the transformed rect (§8). |
| Dock buttons `.barbtn` (Apps, Home, Back, ⋯ + LED) | **reused as is** | Their plates become separate elements (`.plate.dock`). |
| Workspace bar `.wsnum` / `.wsapp` | **reused**, gains a vertical form | In the vertical form the active indicator sits on the left edge, not under the icon. |
| Window sheet (`workspaces::sheet`) | **reused as is** | Opened from the window strip's title as well as by long-press. |
| Quick rail `.rail` | **reused** | Gains the active-face button. Its Fullscreen button becomes the arrangement toggle. |
| Link badge `.linkbadge` / `.lstate` | **reused** | Gains a "words" form for the console. |
| `.tpkey`, `.tptype` (keys) | **reused as is** | Row form (portrait) and the rail's 2-column grid (landscape and desktop). |
| `.tpact` (quick actions) + run/ok/bad states | **reused as is** | Row of tiles (portrait) or column (landscape and desktop). |
| Trackpad surface (`pad_touch`, `pad_pointer`, `pad_gestures`) | **reused**, new mount | Mounted in the deck as a strip with a two-zone click bar (Left, Right). |
| `.npmini` (now playing) | **reused, re-cut** from `.np` | Only in a column deck, under the quick actions. |
| Gamepad clusters (`gamepad.js`) | **reused**, new layout source | The clusters (shoulder-l/r, centre, dpad, face, sticks) already exist. They gain positions from `surround::layout` instead of the overlay's own table while the console is up. |
| **Console bezel** `.plate#bezel` | **new** | s1, r4, holding the rails, the picture and the chin. No shadow, because it does not float. |
| **Window strip** | **new** | Focused app glyph + title + app name (one button → window sheet), Minimise, Maximise/Restore (morphing icon), Close (bad tint, 10 px apart from the others). |
| **Deck** + face switcher `.fbtn` | **new** | Four faces. The active face is accent and a full pill, per the Morphing Toggle Rule. |
| **People chip** + people card | **new (F6)** | Avatars (the device's initial on a participant slot: base0E, 0C, 0A, 09, never the accent or `bad`), the count in mono, and a card listing lead, link, joined time and "End the session for everyone". |
| **Ghost pointer** | **new (F6)** | 14 px dot in the participant's slot with a 5 px halo and a name pill. Idle for more than 3 s: dot only at 50%. Drawn by the client from the `presence` channel. |
| **Arrangement controller** `js/stage_morph.js` | **new** | Snapshot → apply → FLIP (§6). |
| Icons `min`, `winmin`, `winmax`, `restore`, `keys` (arrow-key cluster), `bolt` | **new** in `widgets/icon.rs` | Paths are in the prototype's `ICON` table. |

### 5.2 Copy

- Toggle: **Full screen**. In the cockpit its label is **Leave full screen** (aria-label and title).
- Faces: **Touchpad**, **Keys**, **Quick actions**, **Gamepad**.
- Touchpad hint: "Slide to move · tap to click · two fingers to scroll". Click zones: **Left**, **Right**.
- Window strip: "*Title*" over "*App*". Its buttons read "Minimise *App*", "Maximise *App*" / "Restore *App*", "Close *App*".
- People card: "In this session · 3 of 4 places". Rows read "Pixel 8 · this phone [lead] — Direct · 23 ms · started 40 min ago".
  The note reads "The screen is sized to this phone, because you started the session. Everyone here
  can use the mouse and keyboard." The button reads **End the session for everyone** (it confirms
  next, naming who would be disconnected, per F6).
- Announcements (polite live region): "Full screen. The controls float over the picture." /
  "Console. The controls sit around the picture." / "*Face* below the picture."
- **Rename needed:** the host quick action `fullscreen` is displayed as "Fullscreen" and sits next to
  the client's "Full screen". Change its display name to **"Window full screen"** in
  `server/src/actions/window.rs`. The id stays the same, so no protocol or config change follows.

## 6. Motion

Personality: **calm and heavy**. No overshoot on anything large. `--pop` is kept for things a finger
just flipped. **No new curve.** Every motion uses `--spring` or one of the two named exceptions
(direct manipulation, clocks).

Proposed token: `--dur-stage: 620ms` (to the cockpit) and `--dur-stage-out: 520ms` (back, about 84%).
Reduced: 160 / 160 ms. The motion skill's distance rule asks for this length: the picture travels up
to the full display, about 1.6× the hero transition's distance.

| # | Trigger | What moves | Duration · curve | Reduced / off |
|---|---|---|---|---|
| S1 | Full screen (console → cockpit) | **Picture:** one transform from inset to identity. **Anticipation:** a 1.5% inhale about its centre over the first 14% (≈ 90 ms), then growth, both on `--spring`. Corner radius 16 → 0 (built as four corner caps in ground colour that fade, so it stays compositor-only). | 620 ms `--spring` | Snap, then the new arrangement fades in over 160 ms / instant |
| S2 | same | **Buttons fly** (FLIP, translate plus scale where the size differs) to their cockpit homes, in three waves 24 ms apart: rails and window strip first (nearest the picture's edge, as if pushed), then status, then the dock. Controls with no cockpit place **fold** into the control that absorbs them: the window strip into the focused app icon, the inactive faces into the active face, the name into the link badge, the encoder chip into the LED. They fade out by 38% of the run. | 620 ms `--spring`, stagger ≤ 48 ms | Same as S1 |
| S3 | same | **Plates cross-fade.** The bezel and deck grow with the picture while fading out by 55%. The cockpit's pill plates (dock groups, quick rail) are born from their members' console bounds and fade in from 45%, bringing their dock-float shadow with them. | inside 620 ms | Same as S1 |
| S4 | same | **Ground:** the console's wash fades to stage black, leaving first. | 360 ms accelerate (an opacity fade; DESIGN's named exit) | Same as S1 |
| S5 | same | **Labels:** "Full screen", the link words, the head-count fade (160 ms) while their pills narrow (`clip-path: inset()` in the build; `width` in the prototype). | 160 ms fade + 620 ms `--spring` | Instant |
| S6 | same, Gamepad face | **Clusters fly outward** from the deck or columns to the overlay corners: the d-pad and face buttons first, shoulders and centre in wave 1. The plates behind them gain the float shadow as they cross onto the picture. | 620 ms `--spring` | Same as S1 |
| S7 | Leave full screen (cockpit → console) | Everything reversed, without anticipation: the ground fades back in first (0–55%), cockpit plates fade out by 30%, buttons fly inward, and console plates return from 30%. | 520 ms `--spring` | Same as S1 |
| S8 | Change face (console) | Deck content cross-fades and rises 10 px (row) or slides 10 px in (column). Choosing or leaving Gamepad re-runs the FLIP, because the chin hides and the clusters take its height. | 360 ms / 420 ms `--spring` | Instant |
| S9 | Tap a face in the cockpit rail | Morphs back to the console (S7), where all four faces can be reached. | as S7 | as S7 |
| S10 | Ghost pointer | Follows presence samples (≤ 20 Hz) by linear interpolation over 50 ms (the clock exception). It fades to the idle form after 3 s. Another participant's touch blooms a ring (M18 from the Computer page spec). | 50 ms linear; 520 ms decelerate ring | Snaps; no ring |
| S11 | Someone joins or leaves | The people chip's new avatar pops in (`--pop` scale 0 → 1), the count rolls (`.ms.roll`), and a toast names them. | 300 ms `--pop`, 260 ms `--spring` | Instant |
| S12 | Press on any control | `scale .96` (as today). The window strip's Close also tints `bad` on press. | 120 ms | Unchanged |
| S13 | Maximise ↔ Restore | The icon morphs between one square and two (Morphing Toggle Rule) with a `--pop` dip. | 300 ms `--pop` | Instant |

**Interruptible.** A second tap mid-morph snapshots the *current visual* rects (`getBoundingClientRect`
includes in-flight transforms), cancels the running animations and flips from there. The prototype
does exactly this.

**Never blocks input.** The FLIP is hand-rolled rather than the View Transitions API. VT snapshots are
not interactive, and the picture must take touches from frame one. Pointer and touch mapping read the
video's rect at event time, so a tap mid-morph lands where the finger is.

**Three layers.** Primary: the picture grows. Secondary: buttons are pushed outward in waves, and
plates trade places. Ambient: the link dot's ripple and the ghost pointers carry on throughout.

## 7. Accessibility

- **Same nodes, both arrangements.** Focus, the screen reader's virtual cursor and `aria-pressed`
  survive the morph. The toggle carries `aria-pressed` and swaps its label (Full screen / Leave full
  screen).
- **Landmarks:** the picture is `role="application"` named "*Computer*'s screen". The rails are
  `role="toolbar"` ("Workspaces", "Input"). The window strip is a group named after the window, and
  the deck is a group named "Input". Controls with no place in the current arrangement get `inert`,
  so Tab never lands on an invisible button (the prototype does this).
- **Keyboard:** everything is tabbable, in DOM order: status, workspaces, picture, window strip,
  input, dock. Keys typed while the picture has focus go to the computer, so the client claims no
  shortcuts. Escape leaving browser fullscreen (`fullscreenchange`) morphs back to the console.
- **Targets:** at least 44×44 px everywhere, Close at least 10 px from its neighbours. The deck's
  click zones are 30 px tall but run the full width (≥ 180 px), which is above the WCAG 2.2 AA 24 px
  minimum. Rails are 48 px wide on phones.
- **Contrast:** all chrome sits on s0 or s1 from the slots. Ghost-pointer names use bright text on an
  s1 pill with a slot-coloured border, so the label never relies on the participant colour for
  contrast against an arbitrary picture. Avatars also show an initial, so colour is never the only
  cue. Verify with three schemes (T9).
- **Announcements:** a polite live region carries arrangement and face changes. Joins and leaves
  reuse the toast region.
- **Long-press always has a tap path:** the window strip's title opens the same window sheet that a
  long-press on a dock icon opens.
- **Motion:** `prefers-reduced-motion` and `data-motion` (full / reduced / off) are honoured. Reduced
  mode keeps no travel at all, only a 160 ms fade-in of the new arrangement.

## 8. Engineering notes

- **Input mapping.** `input_units.js` and `viewport.js` already do the letterbox math against the
  video's displayed rect. They must take that rect from `getBoundingClientRect()` (which includes
  transforms), not from `clientWidth` or offsets. Verify with `touch-check` / `tap-check` in both
  arrangements and mid-morph.
- **Cost of a scaled video.** A CSS-scaled `<video>` is a compositor scale and should be free, but
  *measure*: record `decode_drop_pct` and fps on the Pixel in the console and in the cockpit before
  shipping (CLAUDE.md: measure before fixing). No backdrop blur anywhere in the console.
- **The surround adds no work to the input path.** The deck's touchpad is the existing trackpad
  module set, mounted in a different box. Faces re-render only on change.
- **Fullscreen API.** The toggle does both jobs in one gesture: it requests browser fullscreen where
  available (with the orientation lock as today) and morphs. If the request is refused (iOS Safari
  outside a PWA), the morph still happens.
- **State.** `Arrangement { Console, Cockpit }` lives in `live`. The device preferences gain a
  `console` block: `open "console"|"full"` (default `console`) and `face "pad"|"keys"|"quick"|"gamepad"`
  (default `pad` on touch, `quick` on desktop). It is settable by the device itself without the
  settings grant, under the rule `device_trackpad.rs` already follows.

## 9. Build plan (independent tasks)

Each task is one module or file's job, and each merges behind the current cockpit until T6 turns the
console on.

1. **T1 Geometry.** `ui/surround/layout.rs`: a pure function from (viewport, safe insets, output
   size, form, face, arrangement) to (picture rect, a slot rect per control). Unit tests:
   portrait rails hug the picture (390×844 → 270×584), landscape columns, desktop frame, joiner
   letterbox, the software-encode chip replacing the name. No Dioxus types.
2. **T2 State and preferences.** The `Arrangement` signal. The `device_console.rs` schema
   (`open`, `face`) with `ConfigState` / `ConfigSet` and the self-device write rule, mirroring
   `device_trackpad.rs` (tests included).
3. **T3 Picture transform.** `js/stage_fit.js` applies the T1 rect to `#stage-video` as a transform.
   Audit input mapping against transformed rects, and pass `touch-check`, `tap-check` and
   `osk-check` in the console.
4. **T4 Static surround.** `ui/surround/{mod, top_band, window_strip, rails, dock_row}.rs`: place the
   *existing* components from T1's slots. Add the vertical form of the workspace bar and the bezel
   plate. Plates become separate elements.
5. **T5 Deck faces.** `ui/surround/deck/{mod, pad, keys, quick, gamepad}.rs`: mount the trackpad
   modules, `.tpkey`/`.tpact` (via `W.padRail.items`) and `.npmini`. Give `gamepad.js` a `layout`
   source so the clusters take T1's slots.
6. **T6 Morph.** `js/stage_morph.js`: snapshot, apply, FLIP (buttons fly, plates fade, folds,
   anticipation), interruptible, reduced and off modes. Wire `chrome.js`'s `data-fullscreen` to
   toggle the arrangement and listen to `fullscreenchange`. Add `--dur-stage` / `--dur-stage-out`
   to `base.css`. The console becomes the default.
7. **T7 F6 presence and people.** `js/presence.js` (dots from the `presence` channel, 50 ms
   interpolation, idle) and `ui/surround/people.rs` (chip and card). Build against a fixture until
   F6 T5/T9 land.
8. **T8 Docs and small edits.** DESIGN.md: replace "The final arrangement is undecided" with §2–§4
   here; add the gutter law, the plates rule, `--dur-stage`, and the new components. Decision Log
   entry. Rename the `fullscreen` host action's display name. New icons in `widgets/icon.rs`. Add
   "Open sessions in: Console / Full screen" and "Input face" to *On this phone → Appearance → Dock
   & switcher*.
9. **T9 Verify.** Screenshots at 390×844, 844×390, 820×1180 and 1280×800 in default-dark and
   catppuccin-latte. Reduced-motion pass, keyboard pass, contrast on three schemes, and the
   device-drop / fps measurement in both arrangements on the Pixel (§8).

Order: T1 and T2 first, then T3 and T4 together, then T5, T6, T8 and T9. T7 can run any time
(fixture first).

## 10. Screenshots (`shots/`)

All at @2x, opened as `prototype.html?bare&state=…`. The mid-morph frames are frozen with WAAPI at
a fraction *t* of the run. Mid-morph frames are not kept in the repo — open
`prototype.html` and toggle full screen to watch the morph. `--spring` is front-loaded, so *t* ≈ 0.2 already shows most of the
travel.

| File | Viewport | State |
|---|---|---|
| `01-portrait-console.png` | 390×844 | Console, Touchpad face, Ana's pointer live, ThinkPad idle |
| `04-portrait-cockpit.png` | 390×844 | Cockpit (today's dock and rail, plus the people chip) |
| `05-landscape-console.png` | 844×390 | Console, nav column left, Touchpad column right |
| `07-landscape-cockpit.png` | 844×390 | Cockpit |
| `08-desktop-console.png` | 1280×800 | Framed console, Quick actions column with now playing |
| `10-desktop-cockpit.png` | 1280×800 | Cockpit |
| `11-portrait-keys.png`, `12-portrait-quick.png`, `13-portrait-gamepad.png` | 390×844 | The other three faces |
| `14-`, `16-landscape-gamepad-*` | 844×390 | Gamepad: console (both columns are the controller), cockpit overlay |
| `17-portrait-people.png` | 390×844 | People card open |
| `18-portrait-console-light.png`, `22-desktop-console-light.png` | | catppuccin-latte |
| `19-portrait-software-encode.png` | 390×844 | "Software encoding" chip in the band, orange LED |
| `20-landscape-keys.png` | 844×390 | Keys face as the rail's grid |
| `21-tablet-portrait-console.png` | 820×1180 | Tablet portrait |

## 11. Decisions taken (for review)

1. Console by default on every form factor, desktop included. Full screen is one toggle that also
   requests browser fullscreen.
2. **Scale, never resize:** the console transforms the picture and the session's output never
   changes size.
3. Portrait phone: status band on top, workspaces on the left rail, rotate, keyboard and faces on the
   right rail, the window strip as the bezel's chin, the deck under it taking leftover height, and
   the dock row at the bottom as the anchor.
4. Landscape phone: status, workspaces, rotate and keyboard in the top band; window strip under the
   picture; nav, faces and full screen in the left column; the deck in the right column. `flip`
   mirrors the columns.
5. Desktop: a framed window, with the workspace rail on the left and a Quick actions deck column on
   the right; the window strip, nav, faces and ⋯ in the bottom band. No Touchpad face.
6. Tablet: the phone topologies with larger rails and decks.
7. Four input faces (Touchpad default on touch, Quick on desktop). Playback lives in Quick (play/pause
   and track actions plus `.npmini`). Quick tiles stay in the control centre.
8. Window controls: Minimise, Maximise/Restore and Close exist today (`WindowAction`). Free resize is
   **not** shown, because no such action exists ("a feature lands its tile when it lands"). Move uses
   the existing *Move windows* tile.
9. In the cockpit, the faces collapse to the active one on the quick rail. Tapping it returns to the
   console.
10. F6: people chip top-right in both arrangements, avatars in participant slots (never the accent or
    `bad`), ghost pointers drawn as chrome over the picture (they keep their size whatever the scale),
    and the people card as specified.
11. One new duration token pair, `--dur-stage` / `--dur-stage-out`, and no new curve.

## 12. Open questions (not blocking)

1. **Fit the computer to the console?** As an opt-in per device, a session could be started at the
   console's picture size, so text is 1:1 in console mode. The cost: the cockpit would then
   letterbox, or each toggle would need a fresh Output and an IDR. Recommend: not in v1. Revisit
   once T9 measures how small text reads at 69%.
2. **Remember the arrangement per computer** (games stay full screen, desktops in the console)? v1 keeps
   one device-wide `open` preference.
3. **Pinch on the picture in console mode** to zoom the console's scale toward 1:1 around the
   fingers? This would need gesture arbitration with apps' own pinch, so it is deferred.

## 13. Build notes (`2026-10-30`, F5)

What was built, and the calls made while building it that this spec did not settle. Each is
reviewable; none changes an invariant.

**Shape.** `ui/surround/layout.rs` computes the console (T1, unit-tested); `js/stage_fit.js`
writes it as CSS variables (`--c-<slot>-x|r|y|w|h|o`) and `assets/surround.css` places the
controls from them. The cockpit is today's CSS, untouched. `data-arr` on the root is the one
switch, flipped by `js/stage_morph.js`, which snapshots every control, flips it, and flies each
one back (FLIP on `translate`/`scale`, the picture on `transform`). Plates fade by CSS.

1. **Same nodes, placed by CSS rather than by per-button slots.** The dock's groups, the rail's
   buttons, the workspace bar, the link badge and the people chip are positioned in the console by
   `position: fixed` at their slot; FLIP measures leaves, so every button still flies on its own.
   The prototype's per-button slots for the dock (and its plates being born from members' bounds)
   are simplified: plates fade in place.
2. **Every face drives the session, not the computer's own desktop.** The trackpad modules
   (`pad_touch`, `pad_pointer`, `pad_gestures`) send to the daemon's *virtual touchpad on the
   host* (`W.pad`, the `input` grant) — a different screen from the picture. So the deck's
   Touchpad is `js/deck_pad.js`: relative motion on the session's own input channel
   (`pointer_motion`, `button`, `scroll` at a client-tracked pointer), and a pointer dot drawn over
   the picture, since the compositor draws none. Keys (`js/deck_keys.js`) are session key events,
   with the trackpad's latching. The look (`.tpkey`, `.tptype`, `.tpact`) is reused.
3. **Quick actions are session actions plus the computer's player.** `trackpad { quick }` is a
   list of *host* actions run through the pad link; in a session, "next workspace" there would
   switch the host's workspaces. The face holds Previous / Next workspace (the session's) and
   Play / pause plus `.npmini` (the computer's media player, `HostAction::Media`, `host` grant —
   said in words when missing). One list, three doors is not met; revisit with the Virtual input
   work.
4. **Maximise is one button, "Maximise or restore".** `WindowInfo` does not say whether a window
   is maximised, so the icon cannot morph (S13). Adding `maximized` to `WindowInfo` is the fix.
5. **The people chip is F6's** (`js/presence.js`, "N here · names"), placed in the band; no avatar
   stack and no people card. The ghost dots refit when the picture lands (`W.presenceRefit`).
6. **The cockpit rail's face button is its own node** (`.railface`), not the console's face
   button moved; folds (window strip into the dock icon, faces into the active face) are fades.
7. **Gamepad face = a seam.** The pad's box is the `pad` slot (`W.stageFit.slot("pad")`), and
   `surround.css` re-anchors today's clusters for a band (sticks fold away, as in the prototype)
   or a column deck. Choosing the face turns the on-screen pad on, and leaving it turns it off.
   No gamepad settings or visuals were added; the revamp plugs into the slot.
8. **`face` has an `auto` value** (the default) in `device { console { } }`, since the daemon
   cannot know the pointer: Touchpad on touch, Quick on a desktop.
9. **The control centre's Full screen tile** is the same toggle in a session (it was browser
   fullscreen only); outside one it is unchanged.
10. **Targets.** Workspace and rail buttons grow to 44px in the console only; the cockpit keeps its
    denser dock. The link pill stays DESIGN's 32px status target.
11. **The debug readout (`#stagebar`) rides the picture's corner** in the console.

**Not done / not verified.** The decode-drop and fps measurement in both arrangements needs the
Pixel and was not taken (§8): no claim is made about the cost of a scaled `<video>`. Contrast was
checked by eye on default-dark and catppuccin-latte only, not computed across three schemes.
Keyboard and screen-reader passes were not run beyond focus surviving the toggle and `inert` on
hidden controls (checked by `scripts/console-check.mjs`). The prototype's anticipation and wave
stagger are in; corner caps are not (the radius switches with the arrangement).

**Decision Log entry to add to `WADO_PLAN.md`** (untracked here): `2026-10-30` — **Pocket Console
built (F5).** Sessions open as a console by default; Full screen morphs to the cockpit and asks for
browser fullscreen. Scale, never resize: the console is a transform on the video. The deck's faces
drive the session's input channel, not the host's virtual touchpad. Device prefs gain
`device { console { open face } }`, self-settable like `trackpad`. The `fullscreen` host action
displays as "Window full screen".
