# Handoff → Claude Design: wado client UI revamp

**Scope assumption.** The only web UI in this repo is `crates/client` — a Dioxus 0.7
(WASM) single-page app served by the wado server. "Service website" means that app. There
is no separate marketing/landing site; if one was meant, say so, because none exists to
revamp.

**What you are producing.** A visual redesign of that app, as artboards. Not code, not a
refactor plan. Real copy, real states, responsive at every size. The section
[Artboards to draw](#artboards-to-draw) is the deliverable list.

---

## 1. What wado is, and who is holding the screen

wado is a headless Wayland compositor that runs Linux applications on a server and streams
the rendered picture to a browser at low latency, taking mouse, keyboard and touch back the
other way. It is not a mirror of somebody's desktop — apps run *inside* the wado session.
The client app is what you use to configure a session, start it, watch it, and drive it.

The primary device is **a phone**, in both orientations, over mobile data. The secondary
device is a desktop browser. Both run the same markup — there is one component tree with
two presentations, not two designs. Assume the phone is the harder case and the one that
decides every trade-off.

## 2. The governing constraint — read this before drawing anything

**Every piece of UI floats over an arbitrary application being streamed underneath.**

The picture is not your canvas. It is somebody's editor, browser, terminal or game, drawn
by software that knows nothing about you, and it fills the entire stage. So:

- Chrome across the top edge is the single worst place to put anything. That strip is
  where a streamed application draws its own title bar, tabs and menus. Cover it and the
  user cannot see what they are tapping. This is why the status readout is a small corner
  pill that is **off by default**, and why the latency readout lives on the bottom edge —
  the bottom is the strip applications mostly leave alone.
- **Chrome costs picture.** One button opening one panel with tabs beats two buttons and
  two headers. Every pixel of UI is a pixel of the user's actual work that they cannot see.
- Everything floating over the video is `pointer-events: none` — inert, not clickable — with
  exactly two exceptions: the control bar and the console sheet. An overlay that swallowed a
  tap meant for the streamed app would be a worse bug than the one it reported.
- **The stream's aspect ratio is arbitrary.** The output is spawned at the connecting
  device's native resolution — 20:9 phone, 16:10 laptop, anything. No chrome may assume
  16:9 or reserve space by aspect.

A handsome full-width header bar across the top is the default instinct and it makes this
product unusable. Don't.

## 3. Non-negotiables

Each of these came from a real failure. Rule, then what breaks.

| Rule | What breaks without it |
|---|---|
| Nothing conditional appears **above** the video | A banner that comes and goes reflows the stage and continuously rescales the picture as you watch |
| Panels float **over** the picture, never beside or below it | As siblings they take height from the stream and letterbox it — no chosen resolution can fix that |
| Panels hide, they do not disappear from the page | A live terminal, its scrollback and its running shell live inside the console; removing it kills them |
| The software-encoding warning is always shown and cannot be switched off | It is a product invariant: the user must be told the server fell back to software encode. A warning you can turn off is not a warning |
| Touch targets ≥ 44 px; text inputs at exactly 16 px | Below 16 px, iOS Safari zooms the page on focus — which, on a streamed desktop, makes the picture lurch sideways while you type |
| Size in `dvh`, pad with `env(safe-area-inset-*)` | `100vh` overshoots a phone viewport by the URL bar; without the insets the sheet slides under the home indicator |
| Colour never carries meaning alone | The health verdict and the connection stages must read as a word plus a colour. Red/green alone is unreadable to a meaningful share of users |
| The control bar's order is load-bearing | "Close window" is deliberately **not** adjacent to "next window" — that mis-tap is the one that cannot be undone. A separator divides actions-on-the-session from actions-on-this-browser |
| The terminal's interior is not yours to design | A real terminal emulator owns everything inside that box and paints its own text. Design the frame, the tabs and the sizing — never the contents |
| The stage ground stays pure black and is never themed | A light surround bleeds onto the picture and shifts how the stream itself reads |

## 4. Palette contract

The entire UI is coloured through **sixteen base16 variables and nothing else**. Zero
hardcoded hex anywhere. That is what lets a scheme swap be sixteen property writes at
runtime instead of a rebuild.

Four schemes ship (`default-dark`, `gruvbox-dark`, `nord`, `tomorrow-night`) and the user
can paste **any published base16 scheme, including light ones**. So: design against the
variables, and check your work survives a light palette.

Slot meanings — `00` ground · `01` surface · `02` border/selection · `03` dim text ·
`04`/`05` text · `06`/`07` bright text · `08` red · `09` orange · `0A` yellow · `0B` green ·
`0C` cyan · `0D` blue · `0E` magenta · `0F` brown.

`default-dark`, for rendering (these are variable *values*, not colours you may bake in):

```
base00 #181818  base01 #282828  base02 #383838  base03 #585858
base04 #b8b8b8  base05 #d8d8d8  base06 #e8e8e8  base07 #f8f8f8
base08 #ab4642  base09 #dc9656  base0A #f7ca88  base0B #a1b56c
base0C #86c1b9  base0D #7cafc2  base0E #ba8baf  base0F #a16946
```

Current conventions worth keeping or deliberately replacing: green = go/healthy/Start,
red = destructive/Stop/failed, amber = degraded, blue = focus ring and the secondary
action. One shared easing curve and one shared duration for everything that moves —
a single pair is what makes six unrelated transitions read as one interface. Everything
animates via transform/opacity only, so motion never competes with video decode.
`prefers-reduced-motion` kills all of it.

## 5. Surface inventory — with the real copy

Use these strings. A mockup filled with placeholder text is one the user has to refill by hand.

### 5.1 Shell

Two regions. On desktop: a **settings panel** docked left at 320 px beside the **stage**.
Below ~720 px the same panel becomes a **bottom sheet** sliding over the stage, with a
dimming scrim behind it and a grab handle at its top edge.

### 5.2 Settings panel

Header: **wado** / "Configure and start a streaming session."

Then collapsible groups, ordered by *when a setting takes effect* — not by subsystem. Each
group carries a one-line note saying when it applies.

**Connection** (always open, no group chrome)
- `Connection` select: "Direct (HTTP — same LAN / port-forward)" · "Via relay (internet, no port-forward)"
- Direct mode → `Server` text input.
- Relay mode → `Relay URL` (placeholder `ws://my-vps:4000`) and `Remote ID`
  (placeholder `528-491-307 (shown in the server log)`).

**Connection status** (relay mode only) — four ordered hops, each a dot, a name and a hint:
- Relay — "the relay itself is reachable"
- Daemon — "a wado daemon is online for this Remote ID"
- Session — "the compositor and encoder started"
- Video — "media is flowing"

Per-hop states: done (green, solid) · active (amber, pulsing — a stall must look like a
stall, not like a finished state) · failed (red) · pending (grey outline). Below the list,
one of: an error block (red left-rule), "Streaming.", or "Not connected yet — press Start."

**Session** — note: "Read once, at Start." / when live: "Locked while a session is running."
*Every control here is disabled while a session runs.* That is the point of the grouping —
the compositor sizes its output at Start and cannot be resized afterwards.
- `Resolution` select — device-native options first, then "1280 × 720 (720p)", "1920 × 1080 (1080p)", "Custom…". Custom reveals two number inputs side by side.
- `Scale` select — "1× — native (desktop-sized UI)", 1.25×, 1.5×, 1.75×, "2× — phone-friendly", 2.5×, 3×. Long hint below about Hyprland-style monitor scale.
- `FPS` select — 30 / 60 / 90 / 120, each labelled with what it costs **on this screen**: "120 — 50% never shown on this 60 Hz screen", "60 — even on this 60 Hz screen", "60 — uneven on this 90 Hz screen". A conditional hint appears when the chosen rate exceeds the panel's measured refresh.
- `Lock frame rate (like vsync)` checkbox + a three-line hint. Not disabled mid-session, deliberately.
- `Quality` select — "Optimize reactivity (low latency)" · "Balanced" · "Optimize image quality" · "Custom bitrate…". Custom reveals a range slider labelled live: `Bitrate: 6000 kbps`.
- `Encoder` select — "Auto (hardware if available)" · "Hardware only (GPU)" · "Software (x264)".
- `Window placement` select — Center · Top-left · Cascade · Maximized.
- `Focus follows pointer` checkbox.
- `Keyboard repeat` — two number inputs side by side, hint "rate (keys/s) · delay (ms)".
- Nested disclosure **Encoder internals** → `x264 preset` select, `Keyframe interval (frames)` number.

**Live** — note: "Applies immediately."
- Launcher (below).
- `Move-window mode (drag moves windows)` checkbox.
- `Scroll speed: 0.35×` range, label updates live.
- `Natural scroll direction` checkbox.

**Launcher** (inside Live) — `Launch`, hint "Pick an application or type any command.
Spawns into the running session — as many as you like." One text input, placeholder
"search apps, or type a command" — **the command box is the filter**; typing narrows a
list of up to six installed-app rows (app name over its exec line in mono), picking a row
fills the box. The list hides once the box already holds what a row would set. Then a
full-width button: **"Launch into session"**, disabled without a running session.

**Appearance** — note: "Applies immediately." Collapsed by default.
- `Theme` select (the four scheme names). Disabled when a pasted scheme is in force.
- Nested disclosure **Paste a base16 scheme** → hint "Any published base16 scheme — the
  YAML, or just its 16 hex values in order. A pasted scheme overrides the picker; clear the
  box to go back." A mono textarea. Feedback line: "Applied — 16 colours parsed." (green) or
  "Not applied — could not find 16 colours." (red).

**Debug** — note: "View-only. Off also stops the work behind it." Collapsed by default.
Master checkbox `Debug views`; when on it reveals an indented list:
"Status overlay (session, encoder, stats)", "Health verdict (who is at fault)", "Show FPS",
"Show ping", "Show touches / clicks", "Latency breakdown (per stage)".

**Actions** — three equal-width buttons in a row: **Start** (green, disabled when live) ·
**Apply** (disabled when idle) · **Stop** (red, disabled when idle). Below them, a status
line.

### 5.3 Stage overlays

All inert (no pointer events), all translucent with a blur so text survives any picture.

- **Software-encoding banner** — full-width, amber, top edge, animates in:
  "⚠ Software encoding — higher CPU use and latency". Not dismissible. This is the one
  thing allowed at the top edge, because it must be.
- **Status pill** — top-right corner, off by default. Holds an optional status sentence, a
  pipeline badge ("⚡ zero-copy" green / "HW · cpu-copy" amber / "SW · x264" red) and mono
  tabular stats like `58 fps · 27 ms · 41 ms buf`. Drops below the banner when it is present.
- **Health verdict** — top-left, one line, on by default, and **renders nothing at all while
  everything is healthy-and-quiet**. Three states (ok / warn / bad) carried by a coloured dot,
  a bold word, an optional reason, a suggested *fix* in a pill outline, and bandwidth as
  `4.8 / 5.7 Mbps` plus a link figure `↓12.0 Mbps`. This strip exists to name *who is at
  fault* — every other readout is a number, and a number only helps someone who already
  knows what it should be.
- **Latency breakdown** — bottom edge, off by default, a wrapping row of mono chips: a name
  and a tabular value per pipeline stage, plus "dropped" and "device drop %". Fixed-width
  values so digits never jitter horizontally; nothing here animates.

### 5.4 Control bar

Floats bottom-centre over the video — thumb-reachable — and **fades out when idle**, because
everything on it replaces a keyboard shortcut: reachable always, visible rarely. Never fades
while hovered or focused. Rounded, translucent, 44 px square glyph buttons:

`⚙` settings (sheet layouts only) · `⟨`/`⟩` collapse-panel (docked layouts only) ·
`❐` maximize/restore · `—` send to back · `✕` close window · `⇄` next window ·
`❯_` console · `⌨` keyboard (a toggle; shows pressed state) · `⟳` resync video ·
**separator** · `⛶` fullscreen.

Everything left of the separator acts on the session and is disabled without one.
Everything right of it acts on this browser.

### 5.5 Console sheet

One sheet over the picture, rising from the bottom, `min(52dvh, 420px)` tall, rounded top
corners. Two tabs — **Shell** and **Logs** — plus a spacer and a `✕` close. Tabs are 40 px
tall minimum.
- Shell tab: a real terminal emulator fills the body. You design the frame and the tab strip
  only.
- Logs tab: mono log lines, a dim timestamp then the text, coloured by level (ERROR red,
  WARN amber, INFO green, DEBUG/TRACE blue), auto-scrolled to the tail unless the user has
  scrolled up to read history.

One sheet with two tabs, not two panels — a phone has room for one panel at a time and two
collapsed headers is two rows of nothing.

### 5.6 Rejoin prompt

A blocking centred card over the stage, shown when a session is already running. It parks
the connection until answered — there is deliberately no default, because both answers
destroy something a reasonable person might want.

- Title: "A session is already running"
- Description line: "Hardware encoding · vaapi-dmabuf" or "⚠ Software encoding · x264-cpu"
- Hint: "Rejoining keeps its windows and applications, but also its resolution, frame rate
  and scale — the settings on this device are not applied to a session that is already open."
- Two buttons: **"Rejoin it"** (primary) and **"Drop & start new"** (destructive). On a
  narrow screen they must stack into a column — side by side they become thumb-sized targets
  a few millimetres apart, and one of them kills a running session.

## 6. State matrix — draw these, not just the happy path

A revamp that only redesigns a settings panel is half done. These are the states the app
actually lives in:

- **Connection**: not connected · connecting (each of the four hops as the active one) ·
  failed at a named hop with an error message · streaming.
- **Session**: idle (Start enabled, Session group editable) · live (Session group visibly
  locked, Apply/Stop enabled, Launch enabled).
- **Encoder**: hardware zero-copy · hardware cpu-copy · software fallback with the
  permanent banner.
- **Health**: absent · ok · warn · bad (with a pulsing dot and a suggested fix).
- **Rejoin**: prompt blocking the stage.
- **Console**: closed · shell tab · logs tab (including a log full of errors).
- **Launcher**: empty box with no list · six matching rows · settled (box holds an exact
  match, list gone).
- **Panel**: docked open · docked collapsed to zero width · sheet open with scrim · sheet
  shut.
- **Reveals**: custom resolution inputs · custom bitrate slider · encoder-internals
  disclosure · pasted-theme textarea with its applied and not-applied feedback ·
  debug master off vs on with its item list.

## 7. Responsive specification

**This is a hard requirement, not a finishing pass.** The design must hold at every size,
and the layout must adapt rather than scale. Draw the device classes below; state your
breakpoints explicitly in the design.

| Class | Roughly | What must happen |
|---|---|---|
| **Phone portrait** | 360–430 × 800+ | Panel is a bottom sheet (max ~82 dvh) over the stage with a scrim and a grab handle. Touch targets ≥44 px, inputs 16 px. The stagebar's sentence is dropped so the pill stays a pill; the health strip keeps its colour, word and bandwidth and drops the reason and link figure; latency chips shrink rather than disappear — which stage grew is the whole point of them |
| **Phone landscape** | ~800 × 400 | **The forgotten one, and the primary way a phone is held to drive a desktop.** Vertical space is nearly gone: a 52 dvh console leaves almost nothing, an 82 dvh sheet leaves nothing at all. Solve this explicitly — a side sheet, a shorter console, a two-column sheet body. Do not let it fall out of the portrait rules |
| **Small tablet / large phone** | 600–900 | The dock↔sheet crossover currently sits at 720 px. Say where you put it and why |
| **Desktop** | 900–1600 | Panel docked as a 320 px column, collapsible to zero width via the `⟨` bar button — the stage reflows smoothly rather than snapping. Scrim and `⚙` are not present here |
| **Wide desktop** | 1600+ | The panel must not simply stretch. Decide: a wider column, a max-width, or a second column of groups |

Also required at every size:

- The video is `object-fit: contain` on a black ground and the stream's shape is arbitrary.
  Chrome may never depend on the picture's aspect.
- `dvh` for heights, `env(safe-area-inset-*)` for padding against notches and home
  indicators, `viewport-fit=cover` in the meta.
- No horizontal page scroll, ever. Wide content (the latency row, the log lines) wraps or
  scrolls inside its own box.
- The page must not pull-to-refresh on an edge drag — reloading mid-session tears the
  session down, and a downward drag is an ordinary gesture inside the streamed app.
- Sheets and scrollable panels must not rubber-band the page behind them.
- Respect `prefers-reduced-motion`: no transitions, no pulsing dots, no animated reveals.
- Keep the focus ring visible on every control; the video itself is focusable.

## 8. Artboards to draw

1. Desktop — idle, panel docked, all groups collapsed except Connection and Session
2. Desktop — streaming, panel docked, health strip ok, status pill on with stats
3. Desktop — panel collapsed, bar visible, full-bleed picture
4. Desktop — connection failed at the Daemon hop, error shown
5. Phone portrait — sheet open over a live stream, scrim, grab handle, Session group locked
6. Phone portrait — sheet shut, bar visible, health strip in `bad` state with a suggested fix
7. Phone portrait — console open on the Shell tab
8. Phone portrait — console open on the Logs tab with errors
9. **Phone landscape — streaming with the console open** (the space-starved case)
10. Phone portrait — rejoin prompt blocking the stage
11. Phone portrait — software-encoding banner + status pill + latency breakdown all present
12. Launcher with matching app rows
13. Appearance group with a pasted scheme applied, in a **light** base16 palette
14. Component sheet: buttons (primary/destructive/disabled/bar glyph), selects, number
    pairs, range with live label, checkbox rows, group header + note, the four-hop status
    list in all four per-hop states, the health strip in ok/warn/bad, the three pipeline
    badges, log lines at each level

## 9. Out of scope

Don't redesign the terminal's contents, the streamed applications, or the server. Don't add
settings — every control listed above exists because something behind it exists; a new one
in a mockup is a feature request, not a design. Renaming, regrouping, reordering and
rewording are all fair game and welcome.
