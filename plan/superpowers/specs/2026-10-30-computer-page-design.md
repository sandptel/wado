# Computer page + home widgets — design spec

Status: **design, for approval**. No product code changed. Prototype: `prototype.html` (open it directly;
the pills bottom-right switch Light/Dark and Motion full/reduced/off; `?state=…` deep-links every
screenshot state, see §9).

Binding system: `DESIGN.md` (base16 slots, mixed surface ladder, Figtree / Bricolage / JetBrains Mono,
r1–r4, `--spring` / `--pop`). The prototype copies `assets/base.css` tokens verbatim; the light scheme
is `catppuccin-latte` from `schemes.json`. No hex in chrome — the only literal colours are inside the
album-art pictures, which are content.

---

## 1. Who, and in what moment

| Moment | What they need in one tap | Where it lands |
|---|---|---|
| "Use my computer" | the desktop | ▶ on the computer's card (home) — no page in between |
| "Music is playing on the PC, skip / turn down" | the control, not a page | a home **widget** bound to that computer |
| "Do something with *this* computer" | its four doors + its state | **Computer page** (tap the card) |
| "Change how this computer behaves" | its settings, clearly scoped to it | Computer page → **This computer** list |
| "Change how wado feels on this phone" | app settings | home → **On this phone** list |

Principle carried through: *the computer is the content*. Everything about one computer now lives on
that computer's page, named after it; the home page is a lobby of computers plus the few things a
person wants without opening one.

## 2. Information architecture — what moved where

### Home (was: the landing `ui/landing/*`)

Top to bottom: wordmark + tag → **Computers** (cards) → **Running** (sessions) → **On this phone**
(app-wide settings) → **Widgets** (configurable, at the very end, per the decision).

| Was | Now |
|---|---|
| `landing/profiles.rs` computer cards (tap = select, ✎ = Connection page) | Computer cards: tap → **Computer page**; trailing **▶** starts/rejoins the desktop directly. ✎ removed (rename lives in Connection & trust). "Add a computer" kept. |
| `landing/progress.rs` 4-hop ring | Moves into the Computer page hero: while dialling, the LINK cell reads "Connecting…" and the existing 4-hop bar (`.hops`) shows under the meta strip; a failure names the hop that stopped (same logic). Cards show the status words only. |
| `landing/sessions.rs` (Join / New / End) | **Running** on home, each card now says *which computer* (host chip). Join → card ▶ / Desktop action. "New" (a second session) → Display & session → *Start another session*. |
| `ui/media.rs` on the landing | Computer page **Now playing**; and the **Media** widget on home. |
| landing `.shapecard` (resolution summary) | Computer page → Display & session (its subtitle on the list row). |
| landing footer: ⚙ / shell / trackpad / files icon buttons, Start / Join | Computer page **big actions**: Desktop · Trackpad · Files · Shell. ⚙ goes (settings are on home). |
| `landing/clips` clipboard rail | **Clipboard** widget (newest 3, pinned first, *Send this phone's clipboard*); tapping its header opens the existing full rail for that computer. |

### Control-centre pages (`ui/pages/*`, `ui/cc/list.rs`)

| Page today | Goes to |
|---|---|
| Display & stream (`display.rs`) | Computer → **Display & session** (plus the running-session card and *Start another session*) |
| This computer (`host.rs`, config.kdl) | split: stream caps, shortcuts, input-for-everyone → **Display & session** ("For every device on *X*" card, `host` badge); owner-only keys (shells, autostart, trust) → **Connection & trust** |
| Input & touch (`input.rs`) | split: finger mode, scroll, natural scroll → home **Input feel** (this phone); three-finger swipes / pinned repeat (host-saved) → **Display & session** "For every device" |
| Sound (`sound.rs`) + Wi-Fi & Bluetooth (`network.rs`) | Computer → **Sound & devices** (mixer, outputs, Bluetooth devices, Wi-Fi networks with the existing ask-first/probation flow) |
| Connection (`connection.rs`) + per-computer part of Computers & relays (`computers.rs`) | Computer → **Connection & trust** (reached by, Remote ID, relay, rename, forget, this phone's grants, owner-only keys) |
| Trackpad prefs (`device "<key>" { trackpad }`) | Computer → **Trackpad** (landscape, fullscreen, flip, rail, quick actions, gestures, keyboard-on-open) |
| Appearance, Gamepad, Diagnostics, Edit tiles | home **On this phone** (Appearance, Input feel, Gamepad, Quick tiles, Diagnostics) |
| Workspaces & windows (`switcher.rs`: switcher layout, dock pin) | home → Appearance, as a "Dock & switcher" card (it is this phone's chrome, not the computer's) |
| Computers & relays — the saved-relays list | home → Add a computer (the relay picker lives in that flow) |

**In-session control centre** keeps hero, tiles, media, sound card, notifications, shells, clips. Its
11-row settings list shrinks to two rows: **This computer ›** (pushes the same four Computer sub-pages
for the live host inside the sheet) and **On this phone ›**. One component per sub-page, two hosts.

## 3. Screens

**Computer card (home).** 80px, surface-1 + line hairline, r3. Disc (46px, the computer's hue — kept
from `hue(i)`), name (Bricolage 17/700), OS · compositor (mono 11.5), live link words with tone dot,
"· N session". Trailing ▶ 48px circle: accent *tint* normally, solid accent + glow when a session is
running (Rejoin is the likely wish), disabled on an offline computer. Whole left part is one button.

**Computer page.** Sticky bar (back 44px; the name fades/rises into it once the hero scrolls away).
1. **Hero** — r4 surface with the hue/mauve wash; 64px disc; name at 30px (38px desktop); OS ·
   compositor in mono; meta strip of three cells (`.meter` look): **Link** (Direct · 23 ms / Relayed /
   None), **Computer** (Online · up 3 d 4 h / Offline · seen 2 h ago), **Sessions** (count · first app).
   The Link cell is a button: it unfolds the plain-words explanation (`.infotext` pattern).
2. **Big actions** 2×2, 104px tiles: **Desktop** (accent, "Rejoin · Firefox and 2 more" + `running`
   badge, or "Start at this screen's size"), **Trackpad**, **Files**, **Shell** (`2 open` badge; disabled
   with "Not allowed for this phone" without the shells grant).
3. **Now playing** — re-cut `mediacard`: crisp 92px cover on the accent wash (works on light schemes,
   no forced white), app, title, artist · album, scrubber + times, output chip (opens the output list),
   ⏮ ⏯ ⏭ with a 56px play.
4. **Quick** — Volume (thick slider + mute), Brightness (stepped slider, 10% ticks), and three vertical
   toggle tiles: Keep awake · Wi-Fi · Bluetooth (Morphing Toggle Rule; long-press → Sound & devices).
   Wi-Fi's tap opens the existing confirm ("Turn off the computer's Wi-Fi?") — never a one-tap cut.
5. **This computer** — four nav rows, each subtitled with what is set now.

Desktop ≥960px: two columns — hero, actions, This computer | Now playing, Quick (screenshot 13).

**Sub-page** (e.g. Display & session): standard push; page head gains an **eyebrow** with the
computer's hue dot and name above the title, so it is never ambiguous *which* computer is being
changed. Apply bar as today.

**Widgets (home).** Card per widget: header = type label + **host chip** (status dot + computer name;
tap → that computer's page). Bodies: Media (cover, title, artist · output, ⏯ ⏭, thin progress),
Volume (mute + slider), Brightness (stepped slider), Clipboard (3 entries + send), Trackpad (a touch
surface that opens the trackpad for that host + its quick-action chips from `trackpad { quick }`).

**Widget edit mode** (Edit → Done): bodies fold away so every widget is a 64px row and the whole list
fits a screen; each row gets − (remove), type icon, name, host chip as a dropdown (opens the
**host picker sheet**), ↑ ↓, and a ⋮⋮ grip. Below: hint + **Add a widget** tray (all five types; a type
may be added more than once, for different computers).

## 4. Component inventory

| Component | Status | Notes |
|---|---|---|
| `.disc`, `.navrow`/`.list`, `.sect`, `.btn`, `.iconbtn`, `.card`, `.cardhead`, `.whenbadge`, `.seg`, `.switchrow`/`.sw`, `.shape`, `.applybar`, `.sessioncard`, `.appchip`, `.grab`, sheet+scrim | **reused** as is | `.seg` knob moved from `left` to `translate` (compositor-only) |
| `.slide` thick slider | **reused, re-cut** | fill and thumb drawn as layers (`scale`/`translate`) under the native range; adds `grab` / `dragging` states and an optional `ticks` row |
| `mediacard` | **reused, re-cut** as `.np` | crisp cover crossfade layer; progress via `scale` not `width` |
| `.tile` | **reused, variant** `.qt` | vertical, three per row |
| Link badge words / tones | **reused** (`link.rs` labels) | new `.lstate` inline form with the ripple/breathe dot |
| Computer card `.ccard` + ▶ `.cgo` | **new** (replaces `.dev`) | |
| Computer page hero `.hero2` + meta cells `.hcell` | **new** (from `.hero` + `.meter`) | |
| Big action `.act` | **new** | |
| Widget shell `.widget` + host chip `.wfor` + edit row | **new** | |
| Widget bodies (media, volume, brightness, clipboard, trackpad) | **new**, built from reused parts | |
| Host picker sheet | **new** (sheet + navrow + pop check) | |
| Grant card `.grant` (copyable command) | **new** | reuses the "ready-to-paste grant command" wording from `wado tui` |
| Offline card `.offcard` | **new** (from `.alertcard`) | |
| Page-head eyebrow | **new** small addition to `.phead` | |
| Icons `laptop`, `grip`, `minus`, `copy`; filled `prev`/`next` | **new** in `widgets/icon.rs` | |
| Type: **Hero name** 30px/1.05, -0.025em, Bricolage 700 (38px ≥960px) | **new style** | DESIGN.md lists Display as landing-only; add this level between Display and Headline |

## 5. Motion spec

### 5.1 Proposed extension to DESIGN.md's two-curve rule

**No third bezier.** The two curves stay the whole vocabulary. The extension is three tokens and two
named exceptions:

- `--dur-hero: 520ms` (shared-element transitions), `--dur-press: 120ms` (press feedback, today an
  inline literal in base.css), `--stagger: 40ms` (section/list cascade). Reduced: 160ms / 120ms / 0.
- **Direct-manipulation exception:** anything under a finger (a dragged widget, a slider thumb while
  dragging, the trackpad surface) follows the pointer 1:1 with **no easing**; the curve applies only on
  release / on a discrete jump.
- **Clocks and ambient exception:** progress bars run `linear` (they are clocks); live ambient loops
  (the link dot) use `ease-in-out` or a decelerating ripple. Both already exist in `layout.css`
  (`breathe`, `spin`); this names them so they stop looking like violations.
- Exits use the same `--spring` at ~80% of the entrance duration (e.g. 520 → 420ms), and opacity-only
  fades out of departing content may use a plain accelerate (`cubic-bezier(.3,0,1,1)`, ≤180ms) —
  it is a fade, not spatial motion. If that reads as a third curve, drop it and use `--spring`.

### 5.2 Transitions

All transform/opacity only (independent `translate`/`scale` or WAAPI `transform`), never `width`/`top`;
none sets `pointer-events: none` on the destination; every one is interruptible (a second tap reverses
from the current frame — WAAPI `cancel()` + reverse).

| # | Trigger | What moves | Duration · curve | Reduced-motion |
|---|---|---|---|---|
| M1 | Press on any button/card | `scale` .96 (cards .98, ▶ .90); icon wells nudge 2px down | 120ms `--spring`; release 220ms `--pop` | unchanged (it is feedback, ≤120ms) |
| M2 | Tap computer card | **Shared element (FLIP):** card surface → hero surface (`transform` translate+scale from the card rect), disc → hero disc, name → hero name (uniform scale); hero wash fades in (360ms, +120ms); home recedes `scale` .94 + fade (360ms); page ground fades (300ms); bar fades in (+200ms) | 520ms `--spring` (`--dur-hero`) | 160ms crossfade, no movement |
| M3 | Computer page sections enter | each section rises 18px + fades, `--n × 40ms` after 160ms (hero first, then actions, playing, quick, settings) — total ≤ 320ms | 520ms `--spring` | none (instant) |
| M4 | Back from Computer page | M2 reversed: hero shrinks into the card, contents fade first (160ms), home returns from .94 | 420ms `--spring` | 120ms crossfade. If the page was scrolled (>40px) it crossfades instead — the card is no longer near the hero |
| M5 | Link dot, Direct | a soft ring per ping sample (scale 1→2.8, opacity .55→0) — **only when a measurement arrives**; the number rolls up 60% + fades in | ring 2s cycle, decel; number 260ms `--spring` | ring and roll off; the words still say Direct · N ms |
| M6 | Link dot, Relayed / Reconnecting | `breathe` (opacity .35 ↔ 1) | 2.4s ease-in-out | static dot |
| M7 | Link dot, Offline | **no motion** — stillness means nothing is alive | — | — |
| M8 | Slider press / drag / release | press: thumb `scale` 2×.82 + value label `scale` 1.18 in accent; drag: fill and thumb track 1:1; tap-to-jump / keyboard: fill and thumb glide | press 220ms `--pop`; glide 380ms `--spring`; drag none | glide/pop off; value still updates |
| M9 | Toggle tile | Morphing Toggle Rule (radius +8, icon well → circle) + icon dips to .7 and pops back | 360ms `--spring` / 360ms `--pop` | colour change only |
| M10 | Track change | cover: new layer fades in from `scale` 1.08 while the old fades out; title/artist slide out up 8px then in from 10px below | cover 360/520ms; text 160ms out (accel) + 320ms in `--spring` | instant swap |
| M11 | Play/pause | glyph shrinks to .4 and fades, swaps, pops back | 110ms out, 200ms `--pop` in | instant |
| M12 | Progress | `scale` X on its own layer, linear between polls (as today, minus the `width` animation) | linear | updates once a second, no tween |
| M13 | Enter widget edit mode | bodies fold (FLIP keeps neighbours continuous); − bubbles pop in (`scale` 0→1), ↑↓⋮⋮ slide in 24px; stagger 30ms per row; tray rises | 360ms `--pop`; 420ms `--spring`; FLIP 380ms `--spring` | instant |
| M14 | Drag a widget | lift: `scale` 1.03 + Popover shadow (it now floats — Float-Only Shadow Rule holds) + 8ms haptic; others slide out of the way; 4ms haptic per slot crossed; release: settles into its slot with a little overshoot, then DOM reorders under it with no visible jump | lift 200ms `--pop`; make-room 300ms `--spring`; settle 340ms `--pop` | lift without scale; neighbours jump; drop instant |
| M15 | ↑/↓ move, add, remove | FLIP to new order; removed widget shrinks to .9 + fades then the gap closes; added widget pops from .85 | 380ms `--spring`; remove 180ms accel; add 420ms `--pop` | instant |
| M16 | Host picker | sheet slides up (existing `--dur-sheet`); chosen row's check pops; on pick the sheet closes and the widget re-renders with a .96→1 pop | 480ms `--spring`; 300ms `--pop` | 200ms |
| M17 | Sub-page push / pop | existing `push` (100% → 0), page behind drifts −24% and fades | 480ms / 340ms `--spring` | 160ms fade |
| M18 | Trackpad widget touch | a ring blooms from the finger (scale .4→2.2, fade) | 520ms decel | off |

Implementation note for M2/M4: a hand-rolled FLIP (as in the prototype) rather than the View
Transitions API — VT snapshots are not interactive for the transition's length, and the brief says
motion must never block input. With FLIP the live page is under the finger from frame one.

## 6. Widget model

```rust
// wado_protocol — what the phone is told and sends
pub struct WidgetPref { pub kind: WidgetKind, pub order: u16 }
pub enum WidgetKind { Media, Volume, Brightness, Clipboard, Trackpad }
```

Client-side, a widget is `{ id, kind, host_key, order }`; `host_key` is the profile key
(`Profile::key()`), so a widget follows a computer through renames.

**Storage (proposed): per device, on each computer, beside the trackpad block** — a computer stores
the widgets *this device* has bound to *it*:

```kdl
device "pixel-8" {
    trackpad { rail "keys"; quick "workspace-prev" "workspace-next" "fullscreen" "play-pause" }
    widgets {
        widget "media" order=10
        widget "clipboard" order=30
        widget "brightness" order=50
    }
}
```

- Typed `DeviceWidgets` in `crates/config/src/schema/device_widgets.rs` (`deny_unknown_fields`, kinds
  checked against the five), surfaced in `ConfigState`, set through `ConfigSet` by **the device itself
  without the settings grant** (same rule `config::link` already applies to `trackpad`). Live tier,
  written to `ui.kdl`.
- Home order is the **merge of every computer's list by `order`**. Orders are sparse (steps of 10), so
  a reorder rewrites one widget's `order` on one computer (re-spread only when two collide).
- A **local mirror** (`persist.rs`, `wado.widgets`) holds the merged list, so home renders instantly and
  widgets of offline computers keep their place. Writes for an offline computer queue in the mirror and
  flush when it is next reachable; the computer's file wins on conflict.
- Why per computer: a widget is meaningless without its computer, forgetting a computer removes its
  widgets with it, and it survives this phone's browser storage being cleared. The cost is the merge
  above — see open question 1.

Grants per kind: Media/Volume/Brightness need `host`; Clipboard needs `clipboard`; Trackpad needs
`input`. A widget is never hidden for a missing grant — it says what is missing.

## 7. Empty and error states

| State | Computer page | Card / widget |
|---|---|---|
| **Computer offline** | Hero cells: Link *None · nothing to reach*, Computer *Offline · seen 2 h ago*, Sessions —. Red-tinted card: "It is turned off, asleep, or its wado is not running. This page fills in by itself when it is back — nothing to retry." Actions disabled; Now playing and Quick hidden; This computer list stays (Connection & trust must work offline: Remote ID, forget). | Card: red dot, "Offline · seen 2 h ago", ▶ disabled. Widget: dimmed body + "*X* is offline. Seen 2 h ago. This fills in when it is back." |
| **Relayed** | Link cell yellow, breathing; tap explains in plain words ("…nothing on Studio needs fixing"). | "Relayed · limited"; trackpad widget says it plays a little behind the finger. |
| **Connecting / stopped** | Link cell "Connecting…" + the 4-hop bar; on failure, the hop that stopped and its fix (today's `progress.rs` text). | "Connecting…" / "Unreachable". |
| **No host grant** | Now playing: "Needs the host permission — see Quick below." Quick replaced by a grant card with the exact command (`wado allow pixel-8 host`, copy button) and the `wado tui` alternative. Shell action disabled the same way without `shells`. | Widget body: "This phone may not control *X*'s volume. Open its page to see how to allow it." |
| **Nothing playing** | Slim row "Nothing is playing on *X*." (no empty card). | Same line in the Media widget. |
| **No brightness** (desktop, no backlight / no `brightnessctl`) | Row "*X* has no screen brightness wado can set." in place of the slider. | Same in the widget. |
| **In use on another device** | Desktop action "In use on *device*" with Take over (existing seat flow). | Card status "In use on another device" (existing words). |
| **No computers yet** | — | Home shows only "Add a computer" and On this phone; Widgets section hidden until a computer exists. |
| **No widgets** | — | Widgets section shows a dashed "Add a widget" row (opens edit mode with the tray). |

## 8. Build plan (independent tasks)

Each task is one module/file job, mergeable alone behind the current UI until T9 switches the route.

1. **T1 Tokens & motion extension** — `base.css`: `--dur-hero`, `--dur-press`, `--stagger` (+ reduced
   values); DESIGN.md: Hero-name type level, §5.1 rules, new components. `.seg` knob → `translate`.
2. **T2 Icons** — `laptop`, `grip`, `minus`, `copy`, filled `prev`/`next` in `widgets/icon.rs`.
3. **T3 Slider component** — `widgets/slider.rs` (+css): layered fill/thumb over native range, `grab`/
   `dragging` states, optional ticks. Replace the three `.slide` call sites.
4. **T4 Computer page shell + route** — `Page`-like `View::Home | View::Computer(key) | View::Sub(key, page)`
   in `state`; `ui/computer/mod.rs` (dispatcher), `hero.rs`, `actions.rs`, `settings_list.rs`; FLIP
   transition in `js/hero_flip.js` (bridge call, as other JS lives).
5. **T5 Now playing + Quick** — `ui/computer/playing.rs` (from `media.rs`, cover crossfade layer,
   `scale` progress), `ui/computer/quick.rs` (volume, brightness, keep-awake, Wi-Fi with confirm,
   Bluetooth), grant card `ui/computer/grant.rs`, offline card.
6. **T6 Daemon: brightness level** — `HostAction::Brightness { level }` + `HostState.brightness:
   Option<f32>` (brightnessctl get/max); `None` when absent. (Blocks only the slider's true value.)
7. **T7 Sub-pages re-homed** — move/split pages per §2 into `ui/computer/pages/{display_session,
   trackpad, sound_devices, connection_trust}.rs`; app-wide ones into `ui/home/pages/`. Page-head
   eyebrow. In-session CC list reduced to two rows.
8. **T8 Home** — `ui/home/{mod, cards, sessions, settings}.rs` from `ui/landing/*`: computer card with ▶,
   host chip on sessions, On this phone list.
9. **T9 Widgets: config** — `device_widgets.rs` schema + `ConfigState`/`ConfigSet` + self-device write
   rule + tests (mirrors `device_trackpad.rs`).
10. **T10 Widgets: client** — `ui/home/widgets/{mod, shell, media, volume, brightness, clipboard,
    trackpad}.rs`, merge + mirror in `persist.rs`.
11. **T11 Widgets: edit mode** — `ui/home/widgets/edit.rs` (rows, ↑↓, remove, add tray),
    `js/widget_drag.js` (pointer drag, make-room, settle, haptics), host picker sheet.
12. **T12 Multi-host status for widgets** — see open question 2; whichever is chosen.
13. **T13 Verify** — screenshot pass at 390×844 and 1280×800 in default-dark and catppuccin-latte,
    reduced-motion pass, keyboard pass (grip ↑/↓, slider arrows), contrast check on three schemes.

Order: T1–T3 first (shared), then T4 → T5/T7 in parallel, T8, then T9 → T10 → T11; T6 and T12 any time.

## 9. Screenshots (this folder)

390×844 @2x unless noted. Deep link: `prototype.html?bare&state=…`

| File | State |
|---|---|
| `01-home.png` | home: cards (direct / relayed / offline), running session, settings |
| `02-home-widgets.png` | home scrolled to widgets (media, offline volume, clipboard, trackpad) |
| `03a-transition-25.png`, `03b-transition-60.png` | card → Computer page, frozen at 110ms and 260ms |
| `04-computer-dark.png`, `05-computer-dark-quick.png` | Computer page, dark, top and scrolled |
| `06-computer-light.png`, `06b-computer-light-quick.png` | same, catppuccin-latte |
| `07-settings-display.png` | Display & session sub-page with a pending change |
| `08-widgets-edit.png` | widget edit mode |
| `09-widgets-drag.png` | mid-drag: Clipboard lifted over Volume, Volume making room |
| `10-widget-host-picker.png` | host picker sheet |
| `11-computer-relayed-no-host-grant.png`, `11b-…-quick.png` | relayed link explained; no host grant |
| `12-computer-offline.png` | offline computer |
| `13-computer-desktop.png` | Computer page at 1280×800 |

## 10. Open questions

1. **Widget storage**: per computer in `config.kdl` (as proposed, with a local mirror and a merge), or
   simply this phone's local list? The per-computer file costs a merge and offline write-queue.
2. **Live data for widgets of several computers**: today only the selected computer is dialled; others
   are only asked "online?". Should home hold a light control-only link (no media) to each computer that
   has a widget, dropped while a session streams — or do widgets of non-selected computers show
   last-known values until tapped?
3. **Brightness**: add an absolute `Brightness { level }` host action with readback (T6), or ship the
   slider as ±10% steps without a true current value?
4. Approve the **motion extension** in §5.1 (three tokens, two named exceptions, no new curve)?
5. **Wi-Fi's home**: inside *Sound & devices* (proposed, as "the computer's hardware"), or under
   *Connection & trust*?

## Approved by the user — 2026-10-30, with these calls

- Approved as designed, including the motion extension (`--dur-hero`, `--dur-press`, `--stagger`; no new curve; named exceptions: 1:1 finger tracking, clocks/ambient loops).
- **Widgets are stored on the phone** (its own settings, synced via its device prefs), not on each computer.
- **Light live link**: home keeps a control-only link (no video) to each computer that has a widget, so values are live; dropped while a desktop session streams.
- **True brightness**: a daemon action to set brightness to a level and read it back; the slider shows the real value.
- **Wi-Fi lives under Connection & trust** (not Sound & devices).

### Follow-up calls — 2026-10-30, after the first build

- **Light links take no seat — control only.** A widget's link joins with `control=1` (relay cap
  `control`, daemon cap `ctl`): the relay gives it no seat and carries its frames to the daemon
  wrapped by room (`ctl_open` / `ctl` / `ctl_close`, additive in the frozen handshake). The daemon
  serves it its own envelope, only a device it already trusts (no pairing on it), and only the
  control messages — host state and actions, its own config, the clipboard, the action list and
  runs — checking grants per message. The client asks `/online/:id` first and never sends
  `control=1` to a relay that does not say `control`; with an older relay or daemon the widget says
  so and there is no link.
- **One trackpad settings sheet, two doors.** Computer → Trackpad opens the trackpad's own sheet
  (`js/pad_settings.js`, `W.padSettings.openHere`); the second page that wrote the same keys is gone.
- The link badge stays hidden outside a session.
