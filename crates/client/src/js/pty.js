// The console's shell tab: a real terminal, driven by a PTY on the server.
//
// The server sends ANSI, not lines — colour, cursor addressing, the alternate screen an
// editor switches into. xterm.js interprets that; everything here is wiring.
//
// Three things this has to get right, and each was a bug waiting to happen:
//
//   * The emulator is loaded by a <script> tag in index.html while the WASM bundle boots,
//     so it may not exist yet when the bridge runs. Nothing here assumes it does.
//   * A terminal that does not know its size renders wrapped and misplaced the moment
//     anything uses cursor addressing, so the size is measured and re-sent on every change.
//   * The console is hidden with CSS rather than unmounted, so the terminal and its
//     scrollback survive being closed and reopened. Sizing has to be redone on reveal,
//     because a hidden element measures as zero.

(() => {
  let term = null;
  let fit = null;
  let opened = false; // has the server been asked for a shell yet?
  let lastCols = 0;
  let lastRows = 0;

  // Base16-ish, matching the app's palette closely enough not to jar. Not read from CSS
  // variables: xterm wants concrete colours at construction and re-theming a live terminal
  // is not worth the code.
  const THEME = {
    background: "#181818", foreground: "#d8d8d8", cursor: "#d8d8d8",
    black: "#181818", red: "#ab4642", green: "#a1b56c", yellow: "#f7ca88",
    blue: "#7cafc2", magenta: "#ba8baf", cyan: "#86c1b9", white: "#d8d8d8",
    brightBlack: "#585858", brightRed: "#ab4642", brightGreen: "#a1b56c",
    brightYellow: "#f7ca88", brightBlue: "#7cafc2", brightMagenta: "#ba8baf",
    brightCyan: "#86c1b9", brightWhite: "#f8f8f8",
  };

  function sizeChanged() {
    if (!term) return;
    const c = term.cols, r = term.rows;
    if (c === lastCols && r === lastRows) return;
    lastCols = c; lastRows = r;
    W.ptyResize(c, r);
  }

  // Measuring a display:none element gives zero, which fit() turns into a 1x1 terminal
  // that never recovers. Only fit when the element actually has a box.
  function refit() {
    if (!fit || !term) return;
    const el = document.getElementById("wado-term");
    if (!el || !el.clientHeight || !el.clientWidth) return;
    try { fit.fit(); } catch (_) { return; }
    sizeChanged();
  }

  // Build the terminal once the emulator script has arrived. Polled rather than hooked to
  // the script's load event, because by the time this runs the script may already be in.
  function build(then) {
    if (term) { then && then(); return; }
    if (typeof window.Terminal !== "function") {
      setTimeout(() => build(then), 60);
      return;
    }
    const el = document.getElementById("wado-term");
    if (!el) { setTimeout(() => build(then), 60); return; }

    term = new window.Terminal({
      theme: THEME,
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, "DejaVu Sans Mono", monospace',
      fontSize: 13,
      // A phone is narrow and a wrapped prompt is unreadable, so the rows are what give.
      scrollback: 2000,
      cursorBlink: true,
      // The stream already owns the screen; a terminal bell on a phone is noise.
      bellStyle: "none",
      allowProposedApi: true,
      // Touch scrolling in the scrollback rather than the page behind it.
      macOptionIsMeta: true,
    });
    if (window.FitAddon && window.FitAddon.FitAddon) {
      fit = new window.FitAddon.FitAddon();
      term.loadAddon(fit);
    }
    term.open(el);

    // Keystrokes straight through, control characters included — Ctrl-C has to reach the
    // shell as 0x03, not be swallowed as a copy shortcut.
    term.onData((data) => W.ptyInput(data));
    term.onResize(() => sizeChanged());

    // The console is sized in dvh, so a phone's address bar sliding away resizes it.
    if (window.ResizeObserver) {
      new ResizeObserver(() => refit()).observe(el);
    }
    window.addEventListener("resize", () => refit());

    refit();
    then && then();
  }

  // Called when the shell tab becomes visible. Opens the server-side shell the first time.
  W.ptyShow = () => {
    build(() => {
      refit();
      if (!opened) {
        opened = true;
        W.ptyOpen(term ? term.cols : 80, term ? term.rows : 24);
      }
      // Focus after layout, or the keyboard opens against a terminal that is still 0px.
      setTimeout(() => { try { term && term.focus(); } catch (_) {} }, 60);
    });
  };

  // Server output. Buffered until the terminal exists so the shell's own banner — which
  // arrives immediately after pty_open — is not lost to a race with the CDN script.
  const pending = [];
  W.ptyOutput = (data) => {
    if (!term) { pending.push(data); build(() => W.ptyFlush()); return; }
    W.ptyFlush();
    term.write(data);
  };
  W.ptyFlush = () => {
    if (!term || !pending.length) return;
    const queued = pending.splice(0, pending.length);
    for (const d of queued) term.write(d);
  };

  W.ptyExited = () => {
    if (term) term.write("\r\n\x1b[2m[shell exited — reopen the console to start another]\x1b[0m\r\n");
    // Next reveal asks for a fresh shell rather than typing into a dead one.
    opened = false;
  };

  // Forget the terminal entirely — used when the session goes away, so a new session does
  // not inherit the last one's screen.
  W.ptyReset = () => {
    opened = false;
    if (term) { try { term.reset(); } catch (_) {} }
  };
})();
