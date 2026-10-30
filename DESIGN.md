---
name: wado
description: Client chrome for a remote Linux desktop; calm, tactile, honest, and themed entirely from base16.
colors:
  base00: "#181818"
  base01: "#282828"
  base02: "#383838"
  base03: "#585858"
  base04: "#b8b8b8"
  base05: "#d8d8d8"
  base06: "#e8e8e8"
  base07: "#f8f8f8"
  base08: "#ab4642"
  base09: "#dc9656"
  base0A: "#f7ca88"
  base0B: "#a1b56c"
  base0C: "#86c1b9"
  base0D: "#7cafc2"
  base0E: "#ba8baf"
  base0F: "#a16946"
  ground: "var(--base00)"
  surface-1: "color-mix(in oklab, var(--base00), var(--base05) 6%)"
  surface-2: "color-mix(in oklab, var(--base00), var(--base05) 11%)"
  surface-3: "color-mix(in oklab, var(--base00), var(--base05) 18%)"
  line: "color-mix(in oklab, var(--base00), var(--base05) 14%)"
  bright: "var(--base05)"
  text: "color-mix(in oklab, var(--base05), var(--base00) 10%)"
  dim: "color-mix(in oklab, var(--base05), var(--base00) 42%)"
  accent: "var(--base0D)"
  on-accent: "var(--base00)"
  good: "var(--base0B)"
  warn: "var(--base09)"
  bad: "var(--base08)"
  stage: "#000000"
typography:
  display:
    fontFamily: "Bricolage Grotesque, Figtree, system-ui, sans-serif"
    fontSize: "clamp(42px, 12vw, 64px)"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "-0.045em"
  hero-name:
    fontFamily: "Bricolage Grotesque, Figtree, system-ui, sans-serif"
    fontSize: "30px"
    fontWeight: 700
    lineHeight: 1.05
    letterSpacing: "-0.025em"
  headline:
    fontFamily: "Bricolage Grotesque, Figtree, system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 700
    letterSpacing: "-0.01em"
  numeral:
    fontFamily: "Bricolage Grotesque, Figtree, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 600
    lineHeight: 1.1
    fontFeature: "tnum"
  title:
    fontFamily: "Figtree, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 600
  body:
    fontFamily: "Figtree, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.45
  small:
    fontFamily: "Figtree, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
  label:
    fontFamily: "Figtree, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 600
    letterSpacing: "0.1em"
  mono:
    fontFamily: "JetBrains Mono, ui-monospace, SFMono-Regular, Menlo, monospace"
    fontSize: "12px"
    fontWeight: 400
rounded:
  r1: "10px"
  r2: "16px"
  r3: "22px"
  r4: "28px"
  pill: "999px"
spacing:
  xs: "4px"
  sm: "8px"
  gap: "10px"
  md: "12px"
  lg: "16px"
components:
  button:
    backgroundColor: "{colors.surface-2}"
    textColor: "{colors.bright}"
    rounded: "{rounded.r2}"
    padding: "0 16px"
    height: "48px"
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.on-accent}"
    rounded: "{rounded.r2}"
    padding: "0 16px"
    height: "48px"
  button-go:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.on-accent}"
    typography: "{typography.headline}"
    rounded: "{rounded.r3}"
    height: "58px"
  icon-button:
    textColor: "{colors.dim}"
    rounded: "{rounded.pill}"
    size: "44px"
  tile:
    backgroundColor: "{colors.surface-2}"
    textColor: "{colors.bright}"
    rounded: "{rounded.r3}"
    padding: "8px 12px 8px 8px"
    height: "62px"
  tile-on:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.on-accent}"
  card:
    backgroundColor: "{colors.surface-1}"
    textColor: "{colors.text}"
    rounded: "{rounded.r3}"
    padding: "14px"
  nav-row:
    textColor: "{colors.bright}"
    padding: "12px 14px"
    height: "60px"
  input:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.bright}"
    rounded: "{rounded.r1}"
    padding: "8px 12px"
    height: "44px"
  segmented:
    backgroundColor: "{colors.ground}"
    rounded: "{rounded.r2}"
    padding: "4px"
  switch:
    backgroundColor: "{colors.surface-3}"
    rounded: "{rounded.pill}"
    width: "52px"
    height: "32px"
  switch-on:
    backgroundColor: "{colors.accent}"
  link-pill:
    backgroundColor: "{colors.surface-1}"
    textColor: "{colors.bright}"
    rounded: "{rounded.pill}"
    padding: "0 12px 0 10px"
    height: "32px"
  dock-group:
    backgroundColor: "{colors.surface-1}"
    rounded: "{rounded.pill}"
    padding: "5px"
---

# Design System: wado

## Overview

**Creative North Star: "Pocket Console, then Glass Cockpit"**

wado's chrome is the shell around someone else's screen. It works in two phases, and the second
is the one already built. **Glass Cockpit** (incumbent): the streamed desktop owns the whole
display and every control floats over it as an instrument: a thumb-reachable dock that fades when
idle, a link badge in the corner, a control centre that slides up as a sheet. Nothing reflows the
picture. **Pocket Console** (built `2026-10-30`, `ui/surround`; spec
`docs/superpowers/specs/2026-10-30-pocket-console-design.md`): by default a session opens with
the picture drawn smaller at its true aspect ratio, held inside a console shell whose controls
sit *around* it. **Full screen** grows the picture to the whole display, and the same controls
fly outward into the floating cockpit instruments. They are one set of controls in two
arrangements, never two UIs: the dock, quick rail, workspace bar, link badge and people chip are
the same nodes in both.

The arrangement, per form factor: a **top band** of status (link words, the computer's name, the
people chip); the **workspaces** down the left rail (portrait, desktop) or along the band
(landscape); **rotate, keyboard and the four input faces** on the right rail (portrait) or the
left column (landscape); the **window strip** (focused window, Minimise, Maximise, Close) as the
bezel's chin; the **deck** under it (portrait) or beside the picture (landscape, desktop) holding
the active face — Touchpad, Keys, Quick actions or Gamepad; and the **dock row** (Full screen ·
Apps Home Back · ⋯) at the bottom, which stays put through the morph.

The mood is **calm, tactile, honest**. Surfaces are soft, filled and pressable, moving on a spring
with a little mass. The chrome stays quiet until something is wrong, then says so in plain words
and the theme's own warning colours. Personality comes from the user's chosen base16 scheme and
from motion. Decoration is not where it lives.

Density is phone-first and generous: 44–62px targets, 16px input text, safe-area insets on every
edge. The same markup becomes a right-hand card on desktop at 720px.

**Key Characteristics:**
- Every colour comes from sixteen swappable base16 slots; derived tokens are mixes, never hex.
- The stage behind the video is always black, whatever the theme.
- Tonal elevation (a mixed surface ladder) plus soft shadows only on floating chrome.
- Two motion curves for the whole UI: a massy spring and a small overshoot pop.
- The user tunes corner radius, motion level and accent slot from the Appearance page.

## Colors

The palette is the user's base16 scheme. wado assigns *roles* to slots and derives everything
else by mixing in OKLab, so a scheme swap is sixteen property writes with no re-render. The hex
values above are `default-dark`, present only so the page is themed before script runs.

### Primary
- **Accent Slot** (`accent`, base0D by default): selected tiles, switch-on, segmented knob, the
  Start button, focus rings, the active workspace. The user may point it at any slot.
- **On-Accent** (`on-accent`, base00): text and icons placed on the accent.

### Secondary
- **Mauve Wash** (base0E): the second stop in the landing and hero gradient washes, and the
  "host" badge tone. It never appears as a solid fill.

### Tertiary (status)
- **Good** (base0B): direct link, completed steps, the active-session border.
- **Warn** (base09) and **Caution Yellow** (base0A): software-encode chips and in-progress
  steps. The "relayed / limited" and pending-restart tones also take base0A.
- **Bad** (base08): failed steps, alert cards, offline link, danger buttons.

### Neutral
- **Ground** (`ground`, base00): the page and sheet background, and the well inside inputs and
  segmented controls.
- **Surface Ladder** (`surface-1` to `surface-3`): base00 mixed toward base05 at 6 / 11 / 18%.
  Cards and lists sit on 1, tiles and secondary buttons on 2, tracks and grab handles on 3.
- **Line** (`line`, 14% mix): hairline borders and row dividers.
- **Bright / Text / Dim**: titles, body, and secondary text. All three derive from base05,
  the one slot every scheme means as text.
- **Stage Black** (`stage`, #000): behind the video only. It is deliberately unthemed.

### Named Rules
**The Sixteen Slots Rule.** No stylesheet hard-codes a colour. Every value is a base16 slot or a
`color-mix` of slots. The only exceptions are Stage Black and black shadow/scrim mixes.

**The Mixed Ladder Rule.** Elevation is mixed from base00 toward base05, never read from base01
or base02. Schemes disagree about whether base01 is lighter or darker than base00, and only the
mix gives a correct ladder on every scheme, light ones included.

**The Untouchable Picture Rule.** The theme may never change how the stream reads. A light ground
bleeds onto the picture, so the stage stays black.

## Typography

**Display Font:** Bricolage Grotesque (with Figtree, system-ui)
**Body Font:** Figtree (with system-ui)
**Label/Mono Font:** JetBrains Mono

**Character:** A slightly quirky optical-size grotesque for the few loud moments (wordmark, page
heads, live numbers), a friendly geometric sans for everything read, and a mono for anything a
machine produced: addresses, resolutions, workspace numbers, values.

### Hierarchy
- **Display** (700, clamp(42px, 12vw, 64px), 1, -0.045em): the wordmark on the home page only.
- **Hero name** (700, 30px/1.05, -0.025em; 38px from 960px): a computer's name in its Computer
  page hero. Between Display and Headline; nothing else uses it.
- **Headline** (700, 19–20px, -0.01em): control-centre page heads, the hero host name, and the
  Start button at 17px.
- **Numeral** (600, 17px, tabular figures): live meters such as fps, ping and bitrate.
- **Title** (600, 13–14px): tile names, row titles, card heads.
- **Body** (400, 14px/1.45): default text; info text at 12.5px/1.55.
- **Small** (400, 11–12px, dim): row subtitles, hints, field captions.
- **Label** (600, 11px, 0.1em, uppercase): section heads; meter keys at 10px/0.08em.
- **Mono** (400–700, 11–13px): device IDs, resolutions, values, workspace numbers.

### Named Rules
**The Sixteen-Pixel Input Rule.** Every text input is exactly 16px. Below that, iOS Safari zooms
on focus and the streamed picture jumps sideways.

**The Machine Speaks Mono Rule.** A value the system generated (an address, a resolution, a
count) is set in mono. Words a person reads are set in Figtree.

## Layout

The app is one full-viewport stage (`100dvh`, no page scroll) with chrome layered over it in a
fixed z-order: dock (10), landing (40), control-centre scrim (45) and sheet (50), link badge
(65), toasts (70).

- **Dock:** a three-column grid pinned bottom-centre, made of pill groups. The gaps between
  groups pass touches through to the picture. Below 430px, workspaces move to their own row.
- **Control centre:** a bottom sheet at `min(88dvh, 860px)` on phones. From 720px it becomes a
  400px card inset 14px on the right, and the landing slides 210px aside to make room.
- **Landing:** a single centred column, at most 460px wide, with 48px top padding plus the
  safe-area inset.
- **Rhythm:** 4 / 8 / 10 / 12 / 16px. Page blocks sit 12px apart, tiles and cards 8px apart.
  The base gap is 10px.
- **Breakpoints:** 380, 430, 520, 720 and 1100px. Media queries also test `pointer: coarse`
  (bigger targets), `hover: hover and pointer: fine` (turns on blur), and short landscape
  (≤520px tall).
- Every edge respects `env(safe-area-inset-*)`.

**The Picture Owns The Screen Rule.** In the cockpit phase no chrome takes layout space from the
video. Chrome floats over it and only dims what is behind. The Pocket Console phase relaxes this
deliberately: the picture is inset at its true aspect ratio until the user goes full screen.

**The Scale, Never Resize Rule.** The console is a transform on the video, never a different
output size: no fresh Output, no keyframe, no app reflow on a toggle. Input maps against the
video's transformed rect at event time, so a tap lands under the finger in either arrangement and
mid-morph.

**The Gutter Law.** The picture has the display's own aspect, so a band on one axis always leaves
proportional gutters on the other. Each form fills both: a band pair *and* a rail pair. Height
left over goes to the deck, so a taller phone gets a bigger touchpad, never a dead band.

## Elevation & Depth

This is a hybrid system. Depth on content comes from the tonal surface ladder: cards on surface-1
over ground, tiles on surface-2. Shadows appear only on chrome that floats over the video, where
tone alone cannot separate it from an arbitrary picture. Backdrop blur is enabled only on
fine-pointer devices, never on phones, where it costs decode time.

### Shadow Vocabulary
- **Dock float** (`box-shadow: 0 6px 24px -10px rgb(0 0 0 / .55)`): dock pill groups.
- **Sheet lift** (`box-shadow: 0 -20px 60px rgb(0 0 0 / .35)`): the control-centre sheet.
- **Popover** (`box-shadow: 0 18px 50px -20px #000a`): menus and floating cards.
- **Modal** (`box-shadow: 0 40px 100px rgb(0 0 0 / .5)`): the file viewer and other full dialogs.
- **Accent glow** (`box-shadow: 0 10px 30px color-mix(in oklab, var(--accent), transparent 70%)`):
  the Start button only.

### Named Rules
**The Float-Only Shadow Rule.** A shadow means "this is floating over the picture". Content
inside a sheet or card stays flat and gets its depth from tone.

**The Plates Rule.** The surface behind a group of controls (a dock pill, the rail, the console's
bezel and deck) is a plate of its own, never the group's background. In the morph the buttons fly
and the plates only cross-fade, so text never distorts and the shadows arrive with the cockpit's
plates — the moment things start floating over the picture.

## Shapes

The shapes are soft and continuous. A four-step radius scale (10 / 16 / 22 / 28px) nests
outward: an icon well at r2 sits inside a tile at r3, which sits inside a sheet at r4. The scale
is user-switchable through `data-radius`: *sharp* (3 / 4 / 6 / 8px), *round* (the default), and
*pill* (14 / 22 / 28 / 34px). Dock groups, icon buttons, switches, status pills and chips are
always full pills. Borders are 1px hairlines in `line`, and 1.5px dashed for an "add" affordance.
Selection is shown as a 1.5–2px accent border or an accent fill, never as a shadow.

**The Morphing Toggle Rule.** A control that turns on changes shape as well as colour. A tile's
radius grows by 8px and its icon well becomes a circle. A switch's thumb grows from 16 to 24px.

## Components

Every component is soft and pressable: filled tonal surfaces, no hard outlines, a 0.96
press-scale on every tap.

### Buttons
- **Shape:** gently rounded (r2, 16px); full-width by default, 48px tall.
- **Default:** surface-2 fill, bright text, 600 weight.
- **Primary:** accent fill, on-accent text.
- **Danger:** bad at 18% over transparent, bad text. It is a tint, never a solid red.
- **Go (Start):** 58px tall, r3, headline type, accent fill with the accent glow. Used once, on
  the landing.
- **Icon button:** a 44px circle, dim icon, surface-2 on hover.
- **Press / Focus:** `scale: .96` on active; a 2px accent focus outline offset 2px; disabled at
  40% opacity.

### Chips
- **Status pill** (verdict, devstatus, link pill): full pill, tone at 84–86% transparency behind
  tone-coloured text, with a leading 6–8px dot that breathes while the state is live or uncertain.
- **When badge:** 9.5px uppercase pill. *applies live* uses accent, *on Apply* uses base0A,
  *host* uses base0E.
- **App chip:** surface-2 pill, 12px text.

### Cards / Containers
- **Corner Style:** r3 (22px).
- **Background:** surface-1 over ground. Alert cards mix bad into surface-1 at 15% with a bad
  hairline.
- **Shadow Strategy:** none (see the Float-Only Shadow Rule).
- **Border:** a `line` hairline where a card sits directly on the landing wash.
- **Internal Padding:** 14px, with a 10px internal gap.

### Inputs / Fields
- **Style:** a ground-coloured well with a `line` hairline, r1, at least 44px tall, 16px text.
- **Focus:** the border shifts to accent with no outline. A search field adds a 3px accent halo.
- **Field:** a dim 12px caption above, an 11px dim note below.

### Navigation
- **Nav row:** a 60px row inside a surface-1 list (r3) with a tinted 38px icon disc, title and
  subtitle, and a trailing chevron. Rows are divided by `line` at 40%.
- **Page head:** sticky, with a 44px round back button and a 20px headline.
- **Pages** push in from the right on the spring curve; the page behind drifts 24% left and fades.
- **Segmented control:** a ground well with an accent knob that slides on the spring. Segments
  are equal width, so the knob is positioned by `--i / --n`.
- **Switch:** 52×32 pill. The thumb morphs from a 16px dim dot to a 24px on-accent disc.

### Quick Tile (signature)
A 62px, two-column tile: an icon well, then a title and subtitle. Tap toggles; long-press opens the
tile's page. When on, the tile fills with accent and morphs per the Morphing Toggle Rule.
Unavailable tiles sit at 45% opacity and do not respond to input.

### The Dock (signature)
Bottom-centre pill groups at 88% surface-1 with a dock-float shadow. The dock fades to zero
opacity when idle and returns on hover, focus or touch, because it replaces keyboard shortcuts
and must always be reachable but rarely visible. It slides away by `translate` when hidden.

### Link Badge (signature)
A top-left status pill that shows the true link state in plain words, with a tone dot. During a
healthy direct session it collapses to the dot alone (still a 32px target), so a game keeps its
corner. Tapping it opens a card that explains the state and offers a fix.

### Hop Bar
While a computer is being dialled, its Computer page hero shows the four connection hops (Relay,
Daemon, Session, Video) as a bar of four segments under the meta strip. A hop fills good when it
completes; a failure names the hop that stopped and its fix.

### Computer Card and Computer Page (signature)
Home lists computers as 80px cards (surface-1, line hairline, r3): a 46px hue disc, the name in
Bricolage 17/700, OS · compositor in mono, the live link words with a tone dot, and a trailing
48px ▶ that starts or rejoins the desktop directly (accent tint; solid accent with glow while a
session runs; disabled offline). Tapping the card grows it into that computer's page by a
shared-element transition (`--dur-hero`): the card surface becomes the r4 hero with the hue wash,
its disc and name fly to the hero's. The page holds the hero with a three-cell meta strip (Link,
Computer, Sessions), four big actions (Desktop, Trackpad, Files, Shell; 104px tiles), Now
playing, Quick (volume, brightness, three vertical toggle tiles), Clipboard (below) and the four
settings rows for that computer. Its sub-pages carry an eyebrow with the computer's hue dot and name, so a setting
is never ambiguous about which computer it changes.

### Link Words
`.lstate`: a tone dot and plain words. Direct ripples once per ping sample; Relayed and
Reconnecting breathe; Offline is still — stillness means nothing is alive.

### Pocket Console (signature)
The picture inset on an s1 r4 **bezel** (no shadow: it does not float) over a ground wash. Around
it: the **window strip** (a title button that opens the window sheet, then Minimise, Maximise and
Close, Close 10px apart and tinted `bad` on press), the **face switcher** (44px buttons; the
active face is accent and a full pill, per the Morphing Toggle Rule), and the **deck** on an r3
plate. The deck's faces reuse the trackpad's vocabulary (`.tpkey`, `.tptype`, `.tpact`) and the
on-screen gamepad's clusters. **Full screen** is a labelled accent-tint pill in the console and an
icon (`min`, "Leave full screen") on the cockpit's quick rail, where the active face also sits and
leads back to the console. Every target is 44px; the touchpad's Left / Right zones run the deck's
width at 30px.

### Widgets
Home's last section: cards that each follow one computer (header: type label and a host chip
with that computer's status dot). Media, Volume, Brightness, Clipboard, Trackpad. Stored on the
phone. Edit mode folds every body to a 64px row with remove, ↑ ↓ and a drag grip; a dragged
widget lifts (`scale` 1.03 with the Popover shadow — it now floats) and its neighbours make room.
A widget never hides for a missing grant: it says what is missing.

### Clipboard Surface and Chip
One component for a computer's clipboard, in full on its page and short in the Clipboard widget.
Entries are surface-1 cards (r3) with a 38px kind disc (text base0C, link base0E, picture base0A,
each at 20% over transparent), the text in mono clamped to three lines, and a dim meta line saying
where and when it was copied. Actions are 44px circles — Copy (accent tint, the one that matters),
Put back on the computer, Pin, Delete — never hover-only; a tap's glyph pops to a check, a refusal
shakes once. Pinned is a 1.5px accent border, never a shadow. A copy just made arrives with an
accent ring that fades (opacity only); a deleted one folds its row away. Above it, this phone's
two sharing switches for that computer.

Over everything, at the bottom: the clipboard's **voice**, one polite live region. It says what
sharing did in a pill ("Sent to Laptop") and holds the **chip** a browser asks for when only a tap
may use the clipboard: a Popover-shadowed r3 bar that rises on the spring, with a small flow glyph
(computer → phone, or phone → computer) whose accent dot travels the way the copy will go — an
ambient loop that stops under reduced motion — the words, a solid accent Copy/Send and a close.

## Do's and Don'ts

### Do:
- **Do** take every colour from a base16 slot or a `color-mix` of slots; add new roles as mixes
  in `base.css`.
- **Do** keep the stage behind the video `#000`, whatever the scheme.
- **Do** move things only with `transform`/`opacity` or the independent `translate`/`scale`
  properties, so the motion stays off the main thread and away from video decode.
- **Do** use `--spring` (cubic-bezier(.32, .72, 0, 1)) for sheets and pages, and `--pop`
  (cubic-bezier(.34, 1.56, .64, 1)) for small things a finger just flipped. Use `--dur` (220ms)
  and `--dur-sheet` (480ms).
- **Do** time things from the duration tokens: `--dur` (220ms), `--dur-sheet` (480ms),
  `--dur-hero` (520ms, shared-element transitions), `--dur-press` (120ms, press feedback),
  `--stagger` (40ms between sections rising in) and `--dur-stage` / `--dur-stage-out` (620 /
  520ms, the console ⇄ cockpit morph). Reduced motion: 120 / 200 / 160 / 120 / 0 / 160 / 160ms,
  and the morph keeps no travel at all, only a fade of the new arrangement.
- **Do** let exits run on `--spring` at about 80% of the entrance (520 → 420ms); a departing
  opacity-only fade may use a plain accelerate (`cubic-bezier(.3, 0, 1, 1)`, ≤180ms).
- **Do** honour `prefers-reduced-motion` and the user's `data-motion` (full / reduced / off).
- **Do** keep touch targets at 44px or more, and text inputs at exactly 16px.
- **Do** state a degraded link or encoder in plain words with its tone colour, and keep the
  technical name one tap away.
- **Do** make the cockpit and console arrangements of a control the same control, morphing
  between them.

### Don't:
- **Don't** hard-code hex or `rgb()` in chrome. `files.css` currently does (`#888`, `#eee`,
  `#fff`, `rgb(20 20 22 / .94)` in the viewer). That is drift, and it should be brought back to
  tokens.
- **Don't** read elevation from base01 or base02. Mix it.
- **Don't** let chrome reflow the picture in the cockpit phase, or put a banner across it. Health
  lives in the dock LED, a toast or a hero chip.
- **Don't** put shadows on content inside a sheet or card.
- **Don't** enable backdrop blur on coarse-pointer devices.
- **Don't** introduce a third easing curve, or a fourth font family. Two named exceptions are not
  curves: **direct manipulation** (a dragged widget, a slider thumb under the finger, the trackpad
  surface) follows the pointer 1:1 with no easing, and the curve applies only on release or a
  discrete jump; **clocks and ambient loops** (progress bars run `linear`; the link dot's
  `breathe` is `ease-in-out`, its ripple decelerates) are timekeepers, not motion.
- **Don't** show a control that does nothing yet. A feature lands its tile when it lands.
