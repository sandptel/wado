// The console's shells: one xterm per daemon-side shell, each in its own div inside
// #wado-term, which this file owns outright — Dioxus renders that element empty and never
// diffs inside it.
//
// The shells belong to the daemon, not to this page (see server::shells). On every link-up the
// page asks for the list and gets each shell's scrollback back as a `replay`, which *replaces*
// that terminal's screen — so a reconnect redraws rather than duplicates.
//
// Things this has to get right:
//
//   * The emulator is loaded by a <script> tag in index.html and may not exist yet when the
//     bridge runs. Nothing here assumes it does; output that arrives first is queued.
//   * A terminal that does not know its size wraps wrongly the moment anything uses cursor
//     addressing, so size is measured on reveal (a hidden element measures as zero) and on
//     every resize, and re-sent when it changes.
//   * Output goes straight to the emulator, never through a Dioxus signal: it arrives in fast
//     small bursts, and a re-render per burst would make the shell slower than the video.

(() => {
  const terms = new Map(); // id → { term, fit, el, cols, rows }
  const queued = new Map(); // id → [data] that arrived before its terminal could be built
  let active = 0;
  let mods = { ctrl: false, alt: false }; // the key row's sticky modifiers

  const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  // From the live base16 variables, so the terminal follows the scheme.
  const theme = () => {
    const b = (n) => css("--base0" + n) || undefined;
    return {
      background: b("0"), foreground: b("5"), cursor: b("5"), selectionBackground: b("2"),
      black: b("0"), red: b("8"), green: b("B"), yellow: b("A"), blue: b("D"),
      magenta: b("E"), cyan: b("C"), white: b("5"),
      brightBlack: b("3"), brightRed: b("8"), brightGreen: b("B"), brightYellow: b("A"),
      brightBlue: b("D"), brightMagenta: b("E"), brightCyan: b("C"), brightWhite: b("7"),
    };
  };
  W.ptyRetheme = () => { for (const t of terms.values()) { try { t.term.options.theme = theme(); } catch (_) {} } };
  {
    const set = W.setTheme;
    W.setTheme = (...a) => { set(...a); W.ptyRetheme(); };
  }

  const send = (obj) => W.relaySendMsg && W.relaySendMsg(obj);
  const host = () => document.getElementById("wado-term");

  function sized(id) {
    const t = terms.get(id);
    if (!t || !t.fit || !t.el.clientWidth || !t.el.clientHeight) return;
    try { t.fit.fit(); } catch (_) { return; }
    if (t.term.cols !== t.cols || t.term.rows !== t.rows) {
      t.cols = t.term.cols; t.rows = t.term.rows;
      send({ type: "pty_resize", id, cols: t.cols, rows: t.rows });
    }
  }

  // The terminal for `id`, built on first use. Returns null until xterm has loaded.
  function ensure(id) {
    if (terms.has(id)) return terms.get(id);
    if (typeof window.Terminal !== "function" || !host()) return null;
    const el = document.createElement("div");
    el.className = "termpane";
    el.hidden = id !== active;
    host().appendChild(el);
    const term = new window.Terminal({
      theme: theme(),
      fontFamily: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
      fontSize: 13,
      scrollback: 4000,
      cursorBlink: true,
      bellStyle: "none",
      allowProposedApi: true,
      macOptionIsMeta: true,
    });
    let fit = null;
    if (window.FitAddon && window.FitAddon.FitAddon) { fit = new window.FitAddon.FitAddon(); term.loadAddon(fit); }
    term.open(el);
    // Keystrokes straight through, control characters included — Ctrl-C reaches the shell as
    // 0x03. The key row's sticky Ctrl/Alt apply to the next character typed.
    term.onData((data) => send({ type: "pty_input", id, data: withMods(data) }));
    const t = { term, fit, el, cols: 0, rows: 0 };
    terms.set(id, t);
    if (window.ResizeObserver) new ResizeObserver(() => id === active && sized(id)).observe(el);
    for (const d of queued.get(id) || []) term.write(d);
    queued.delete(id);
    return t;
  }

  function withMods(data) {
    if (!mods.ctrl && !mods.alt) return data;
    let out = data;
    if (mods.ctrl && data.length === 1) {
      const c = data.toUpperCase().charCodeAt(0);
      if (c >= 64 && c <= 95) out = String.fromCharCode(c - 64);
    }
    if (mods.alt) out = "\x1b" + out;
    mods = { ctrl: false, alt: false };
    emit({ type: "shellMods", ctrl: false, alt: false });
    return out;
  }

  // Wait for xterm, then run `f`.
  function ready(f) {
    if (typeof window.Terminal === "function" && host()) f();
    else setTimeout(() => ready(f), 60);
  }

  W.shellShow = (id) => {
    active = id;
    ready(() => {
      ensure(id);
      for (const [k, t] of terms) t.el.hidden = k !== id;
      // After layout, or the fit measures a box that is still 0px.
      requestAnimationFrame(() => { sized(id); const t = terms.get(id); try { t && t.term.focus(); } catch (_) {} });
      setTimeout(() => sized(id), 150);
    });
  };
  // Re-fit the visible terminal; the console calls this when it is revealed.
  W.ptyShow = () => { if (active) W.shellShow(active); };

  W.shellNew = (alias) => {
    const t = terms.get(active);
    send({ type: "pty_open", cols: (t && t.cols) || 80, rows: (t && t.rows) || 24, host: alias || null });
  };
  W.shellClose = (id) => send({ type: "pty_close", id });
  W.shellsRequest = () => send({ type: "shells_request" });

  // The key row: what a phone keyboard does not have.
  const KEYS = { esc: "\x1b", tab: "\t", up: "\x1b[A", down: "\x1b[B", right: "\x1b[C", left: "\x1b[D",
    home: "\x1b[H", end: "\x1b[F", pgup: "\x1b[5~", pgdn: "\x1b[6~", pipe: "|", tilde: "~", slash: "/", dash: "-" };
  W.shellKey = (name) => {
    if (!active) return;
    if (name === "ctrl" || name === "alt") {
      mods[name] = !mods[name];
      emit({ type: "shellMods", ctrl: mods.ctrl, alt: mods.alt });
      return;
    }
    const seq = KEYS[name];
    if (seq) send({ type: "pty_input", id: active, data: withMods(seq) });
    const t = terms.get(active);
    try { t && t.term.focus(); } catch (_) {}
  };

  W.relayOn("shells", (msg) => {
    const shells = msg.shells || [];
    // A tab closed here or elsewhere: its terminal goes too.
    for (const [id, t] of terms) {
      if (!shells.some((s) => s.id === id)) { try { t.term.dispose(); } catch (_) {} t.el.remove(); terms.delete(id); }
    }
    if (!shells.some((s) => s.id === active)) active = shells.length ? shells[shells.length - 1].id : 0;
    emit({ type: "shells", shells, hosts: msg.hosts || [], active });
  });
  W.relayOn("pty_opened", (msg) => { emit({ type: "shellActive", id: msg.id }); W.shellShow(msg.id); });
  W.relayOn("pty_output", (msg) => {
    const id = msg.id || 0;
    if (!id) return;
    const t = ensure(id);
    if (!t) {
      const q = queued.get(id) || [];
      if (msg.replay) q.length = 0;
      q.push(msg.data || "");
      queued.set(id, q);
      ready(() => ensure(id));
      return;
    }
    if (msg.replay) t.term.reset();
    t.term.write(msg.data || "");
  });
  W.relayOn("pty_exit", (msg) => {
    const t = terms.get(msg.id || 0);
    if (t) t.term.write("\r\n\x1b[2m[exited — close this tab, or open another]\x1b[0m\r\n");
  });

  // Ask for the shells on every link-up: they may have been running all along.
  {
    const up = W._relayHandlers.__up;
    W.relayOn("__up", (m) => { if (up) up(m); W.shellsRequest(); });
  }
})();
