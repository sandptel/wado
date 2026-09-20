# UI — the client

Dioxus 0.7, web/WASM, built with `dx`. Native Dioxus owns state and rendering; everything
browser-only lives in `crates/client/src/js/` and is driven through `bridge.rs`.

**Verification in this lane is a human looking at it.** Byte checks prove the code shipped
and nothing more — a CSS change that made the video paint over the panels passed byte
verification and was completely broken.

## Layout rules learned the hard way

Each of these came from a user report.

- **Nothing conditional above the video.** A banner that appears and disappears reflows the
  stage and rescales the picture continuously. ("your message at the top is rescaling the
  view all the time")
- **Panels must float over the picture, never sit beside it.** As siblings they take height
  from the stream and letterbox it. `#console` is `position: absolute` over `#stage-video`.
- **`#stage` is a flex column; the video lives in its own `#stage-video` wrapper** with
  `flex: 1; min-height: 0`, and `#wado-video` is `position:absolute; inset:0` inside it.
  Removing the wrapper is what made the video paint over the panels.
- **Hide panels with a class, never unmount them.** The terminal, its scrollback and the
  running shell all live inside `#console`; unmounting kills them. `#console.shut { display:none }`.
- **Chrome costs picture.** One button opening one panel with tabs beats two buttons and two
  headers. Designed for a phone first.
- Touch targets ≥40 px. Inputs at **exactly 16px** or iOS Safari zooms the page on focus,
  which on a streamed desktop makes the picture jump sideways as you type.
- Size in `dvh`, pad for `env(safe-area-inset-*)`, and keep `viewport-fit=cover` in the
  meta — without it the safe-area insets resolve to zero.

## The JS bridge

- `bridge.rs` concatenates `js/*.js` with `include_str!` **in order**. `core.js` defines the
  shared `W` object and must be first; `lifecycle.js` must be **last** (it ends in a
  never-resolving await that keeps the eval and its `dioxus.send` channel alive).
- It is one eval, so it **cannot await a script tag**. A CDN library goes in `index.html`,
  and the bridge module waits for the global rather than assuming it (`pty.js` polls for
  `window.Terminal` and buffers output that arrives first).
- Rust → JS is one-shot evals into `window.__wado.*`; JS → Rust is `dioxus.send`.
- High-rate data must bypass signals. PTY output goes straight to the emulator — routing it
  through a re-render would make the shell feel slower than the video.

## State traps

- Two controls sharing one signal is a real bug that shipped: the console input and the
  sidebar launcher were both `Settings::command`, so typing in one rewrote the other. They
  look alike and are not — a saved app to launch vs a line being typed now.
- A saved value must not outlive the options that offer it. A restored `res` matching no
  `<option>` renders as a blank select; there is an effect that replaces it once both the
  saved blob and the device screen are known.
- The `loaded` gate before persisting is load-bearing — without it the first effect run
  writes defaults over the saved blob.
- A native `<details>` toggling is not a state change. It needs a click handler or the next
  render closes it.

## The app drawer (since 2026-09-19)

Launching used to live in the settings panel: open panel → scroll past the session group →
type → Launch. Four deliberate actions for the thing people do most. It is now ⊞ on the bar →
a bottom sheet over the video (`client/src/ui/drawer/`), same sheet shape as the console.

- **Tap launches and closes; long-press fills the command box.** Long-press is `oncontextmenu`
  — what a touch long-press and a desktop right-click both raise. No timers, no pointer
  bookkeeping. Tiles need `user-select:none` and `-webkit-touch-callout:none` or the browser
  starts a selection or an image drag under the press.
- **The box is the filter and the command line**, as the old launcher's was. One input.
- **Recents are stored as bare `Exec` strings** and re-joined with the live list at render, so
  an entry cannot go stale and a hand-typed command still comes back.
- The settings panel keeps only the free-text box (`ui/launcher.rs`).

### The running dot

`AppEntry.running` is filled in when the list is *answered*, not when it is discovered: the
server asks the compositor (`CompositorCommand::RunningApps`) which launched commands still
have live processes, and joins on the exact command string — the same text the client sent, so
no name has to be guessed. Refreshed on every drawer open, which is also the only time it can
have changed and someone is looking.

**It means the process is alive, not that a window is mapped.** Ordering makes it reliable
anyway: launch and the apps request both travel the same compositor channel, so the launch is
always handled first.

### What the drawer lists, and what it buries

**The server discovers *launchable*, the client decides *listable*.** `parse_entry` used to
drop every entry marked `NoDisplay`/`Hidden`; it now carries them as `AppEntry.hidden`.
Measured here: **143 launchable entries, 71 of them `NoDisplay`** — MIME handlers, setup
helpers, per-scheme stubs. Half the menu is noise, and the other half is what someone came for.

Default list is `!hidden && icon.is_some()`; 👁 on the search row lists everything and the
button says how many more that is. The missing-icon half of the rule is a heuristic, not a
fact — it catches packaging leftovers that forgot `NoDisplay` — which is exactly why the eye
overrules both marks rather than just one.

⚠️ **`MAX_TILES` was 40 and it read as "apps are missing".** It was a DOM guard doing duty as
curation: 72 listable apps, 40 rendered, no indication the other 32 existed. 400 now. A cap
that silently truncates needs to say so or be high enough never to bite.

**Payload: 869 KB per drawer open** with the hidden half carried (645 KB before). Every open
re-fetches. The upgrade path if this starts hurting on mobile data is a list hash the client
can send to skip the body — not incremental icons.

### Icons ride inside the app list

`AppEntry.icon` is a `data:` URI, not a name or a path: relay mode has no HTTP route back to
the server, and the browser cannot read the server's filesystem. Resolution is
`server/src/apps/icons/` — one cached walk of every `icons`/`pixmaps` root, best size bucket
per name, 32 KiB cap per file. **Measured here: 72 apps → 63 icons → 661 KB of JSON.**

⚠️ **`Path::file_stem("org.gnome.Calculator")` is `"org.gnome"`.** Icon names are reverse-DNS
more often than not, so stripping "the extension" with `file_stem` silently lost every GNOME
app's icon. Strip only a known image extension.

⚠️ **The size bucket is the *grandparent* directory** (`hicolor/48x48/apps/x.png`), not the
parent. Reading the parent scored every icon identically and the "best fit" logic did nothing.

## The gamepad layout is browser-owned, and clusters are the unit

`2026-09-20`. Edit mode used to send the layout `JS → emit → Dioxus signal → persist::Saved →
localStorage → restore → apply`, and it **did not survive a reload**. A Node harness proved the
JS half of the round trip correct, so the break was in the Rust hop — and that hop was deleted
rather than debugged. `js/gamepad.js` owns the layout end to end under its own key
(`wado.padLayout`), read at module init and written on every drag. `pad_layout` is gone from
`state.rs`, `persist.rs` and `bridge.rs`; `pad_edit` stays in `Live`, because edit mode is
session state and the checkbox needs it.

**What moves is the cluster, not the control.** `data-cluster` already existed for the CSS
anchors; edit mode keys off it. A D-pad whose arms drag apart is four buttons, not a cross. One
`lo`/`hi` clamp is computed from the min/max member fraction so the group stays rigid against a
stage edge, and `resize` spreads members about the cluster centroid — arms are *positions*, not
padding, so scaling the buttons alone just makes them overlap.

⚠️ **Press feedback uses the independent `scale:` property, never `transform: scale()`.**
`transform` is where the layout lives (the D-pad arm translates, and edit mode writes an inline
`translate(-50%,-50%)`), so a transform-based press state is silently cancelled on any dragged
control. Do not "simplify" it back.

**The check is `scripts/padlayout-check.mjs`** — `node scripts/padlayout-check.mjs`, no
framework. It loads the shipped `gamepad.js` against a DOM shim and asserts the four arms move
by one delta, resize spreads and scales, a fresh page reads the layout back out of storage, and
reset clears both. Harness gotcha: `#wado-pad` needs its own entry in `RECTS` or every drag
clamps to one pixel — an artefact that reads exactly like a clamping bug.
