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
