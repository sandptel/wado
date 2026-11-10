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
  // The type size, pinched or stepped, kept on this device. Half points: steady under fingers.
  const FONT_MIN = 8, FONT_MAX = 24;
  let font = (() => { try { return Math.min(FONT_MAX, Math.max(FONT_MIN, +localStorage.getItem("wado.shell_font") || 13)); } catch (_) { return 13; } })();

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
    // Again once the scheme has eased in: mid-way, the variables read half-way colours.
    W.setTheme = (...a) => { set(...a); W.ptyRetheme(); setTimeout(W.ptyRetheme, 500); };
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
      fontSize: font,
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

  // The key row: what a phone keyboard does not have. A name not here is itself the character.
  const KEYS = { esc: "\x1b", tab: "\t", up: "\x1b[A", down: "\x1b[B", right: "\x1b[C", left: "\x1b[D",
    home: "\x1b[H", end: "\x1b[F", pgup: "\x1b[5~", pgdn: "\x1b[6~", pipe: "|", tilde: "~", slash: "/", dash: "-",
    "^c": "\x03", "^d": "\x04", "^z": "\x1a", "^r": "\x12", "^l": "\x0c", "^a": "\x01", "^e": "\x05", "^w": "\x17", "^u": "\x15" };
  W.shellKey = (name) => {
    if (!active) return;
    if (name === "ctrl" || name === "alt") {
      mods[name] = !mods[name];
      emit({ type: "shellMods", ctrl: mods.ctrl, alt: mods.alt });
      return;
    }
    const seq = KEYS[name] || (name.length === 1 ? name : "");
    try { if (navigator.vibrate) navigator.vibrate(4); } catch (_) {}
    if (seq) send({ type: "pty_input", id: active, data: withMods(seq) });
    const t = terms.get(active);
    try { t && t.term.focus(); } catch (_) {}
  };

  // ── The actions row ───────────────────────────────────────────────────────────────────────
  const cur = () => terms.get(active);
  const note = (text) => emit({ type: "captureNote", text });
  // The phone keyboard: up if it is down, down if it is up.
  W.shellKeyboard = () => {
    const t = cur();
    if (!t) return;
    const typing = t.el.contains(document.activeElement);
    try { typing ? t.term.blur() : t.term.focus(); } catch (_) {}
  };
  W.shellPaste = async () => {
    const t = cur();
    let text = "";
    try { text = await navigator.clipboard.readText(); } catch (_) {}
    if (!t) return;
    if (!text) { note("Nothing to paste — or this browser kept the clipboard to itself."); return; }
    t.term.paste(text);
  };
  W.shellCopy = async () => {
    const t = cur();
    const text = t ? t.term.getSelection().replace(/\s+$/, "") : "";
    if (!text) { note("Select text in the terminal first, then Copy."); return; }
    try { await navigator.clipboard.writeText(text); note(`Copied ${text.length} characters.`); } catch (_) { note("This browser would not let the page copy."); }
    t.term.clearSelection();
  };

  // ── Zoom: pinch the terminal, or step it ──────────────────────────────────────────────────
  const badge = () => {
    let b = document.getElementById("termzoom");
    if (!b && host()) { b = document.createElement("div"); b.id = "termzoom"; host().appendChild(b); }
    return b;
  };
  let badgeTimer = 0;
  function setFont(f, keep) {
    f = Math.min(FONT_MAX, Math.max(FONT_MIN, Math.round(f * 2) / 2));
    const b = badge();
    if (b) { b.textContent = f + " pt"; b.classList.add("on"); clearTimeout(badgeTimer); badgeTimer = setTimeout(() => b.classList.remove("on"), 700); }
    if (f !== font) {
      if (Math.abs(f - font) >= 1) { try { if (navigator.vibrate) navigator.vibrate(3); } catch (_) {} }
      font = f;
      for (const [id, t] of terms) { try { t.term.options.fontSize = f; } catch (_) {} if (id === active) sized(id); }
    }
    if (keep) { try { localStorage.setItem("wado.shell_font", String(font)); } catch (_) {} }
  }
  W.shellZoom = (d) => setFont(font + d, true);
  {
    const fingers = new Map();
    let from = 0, spread0 = 0;
    const spread = () => { const [a, b] = [...fingers.values()]; return Math.hypot(a.x - b.x, a.y - b.y); };
    const on = (type, f) => document.addEventListener(type, (e) => { if (host() && host().contains(e.target)) f(e); }, { capture: true, passive: false });
    on("touchstart", (e) => {
      for (const t of e.changedTouches) fingers.set(t.identifier, { x: t.clientX, y: t.clientY });
      if (fingers.size === 2) { from = font; spread0 = spread(); }
    });
    on("touchmove", (e) => {
      for (const t of e.changedTouches) if (fingers.has(t.identifier)) fingers.set(t.identifier, { x: t.clientX, y: t.clientY });
      if (fingers.size === 2 && spread0 > 0) { e.preventDefault(); setFont(from * spread() / spread0, false); }
    });
    const end = (e) => {
      for (const t of e.changedTouches) fingers.delete(t.identifier);
      if (fingers.size < 2 && spread0) { spread0 = 0; setFont(font, true); }
    };
    on("touchend", end);
    on("touchcancel", end);
  }

  // ── The phone keyboard: the sheet rides above it ──────────────────────────────────────────
  // The layout viewport does not shrink for the keyboard (iOS, Chrome since 108), so the sheet
  // would sit under it. The visual viewport says how much is left; the sheet takes that.
  if (window.visualViewport) {
    const vv = window.visualViewport;
    const fit = () => {
      const kb = Math.max(0, window.innerHeight - vv.height - vv.offsetTop);
      const root = document.documentElement;
      root.style.setProperty("--kb", kb + "px");
      root.style.setProperty("--vvh", vv.height + "px");
      const c = document.getElementById("console");
      if (c) c.classList.toggle("kb", kb > 80);
      if (active) requestAnimationFrame(() => sized(active));
    };
    vv.addEventListener("resize", fit);
    vv.addEventListener("scroll", fit);
  }

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
