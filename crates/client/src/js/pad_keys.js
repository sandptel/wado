// wado bridge — the trackpad's keyboard: the phone's own keyboard (autocorrect, swipe, dictation)
// typing into the computer, and a modifier row for shortcuts.
//
// Text: the field is diffed against what was already sent — by code point, not UTF-16 unit — and
// only the difference goes out: backspaces for what was taken back, then the new text. That is
// what makes autocorrect and dictation corrections land right ("teh" → "the " is two backspaces
// and "he "). A composition (Gboard's whole current word, a dictated phrase) is sent when it ends.
//
// Sentinel: the field always keeps one zero-width space at the start, never sent and excluded
// from the diff. On Android Gboard, Backspace on an otherwise-empty field arrives as
// `key === "Unidentified"` (keyCode 229) and fires no `input` event, so an empty field would
// never tell us it happened. The sentinel gives it something to delete: when a flush finds the
// sentinel gone, that deletion *was* the backspace — one `key: "backspace"` goes out and the
// sentinel is restored.
//
// Modifiers: tap → latched for the next key; double-tap (300 ms) → locked (held on the computer
// until tapped again, so Super + drag can move a window); tap a locked one → off. A latched
// modifier turns the next single character from the keyboard into a shortcut (`char`) instead of
// text, so "Super, then 1" switches workspace.
//
//   W.padKeys.attach(textarea, row) / detach()
//   W.padKeys.diffText(prev, next) → {back, ins}
//   W.padKeys.mods  {super, ctrl, alt, shift} each "off" | "latched" | "locked"
//   W.padKeys.tapMod(name), W.padKeys.key(name), W.padKeys.onMods(fn), W.padKeys.repaint()

(() => {
  const K = (W.padKeys = {});
  const NAMES = ["super", "ctrl", "alt", "shift"];
  const DOUBLE_MS = 300;
  // Never sent as text, never part of the diff — see the sentinel note above.
  const SENTINEL = "​";
  // ponytail: the field is a 200-code-point window; past that it clears, which keeps autocorrect
  // working on the current words without the field growing forever.
  const WINDOW = 200;
  K.mods = { super: "off", ctrl: "off", alt: "off", shift: "off" };
  const modFns = [];
  const lastTap = {};
  let input = null;
  let sent = "";
  let composing = false;

  const cps = (s) => Array.from(s);
  K.diffText = (prev, next) => {
    const a = cps(prev), b = cps(next);
    let i = 0;
    while (i < a.length && i < b.length && a[i] === b[i]) i++;
    return { back: a.length - i, ins: b.slice(i).join("") };
  };

  const send = (m) => W.pad.send(m, false);
  const inState = (st) => NAMES.filter((n) => K.mods[n] === st);
  K.onMods = (fn) => modFns.push(fn);
  const paint = () => { for (const fn of modFns) { try { fn({ ...K.mods }); } catch (_) {} } };
  K.repaint = paint;
  function clearLatches() {
    let any = false;
    for (const n of NAMES) if (K.mods[n] === "latched") { K.mods[n] = "off"; any = true; }
    if (any) paint();
  }

  K.tapMod = (name) => {
    const t = performance.now();
    const double = t - (lastTap[name] ?? -1e9) < DOUBLE_MS;
    lastTap[name] = t;
    const st = K.mods[name];
    if (st === "locked") { K.mods[name] = "off"; send({ k: "hold", mods: inState("locked") }); }
    else if (st === "latched" && double) { K.mods[name] = "locked"; send({ k: "hold", mods: inState("locked") }); }
    else if (st === "latched") K.mods[name] = "off";
    else K.mods[name] = "latched";
    paint();
  };

  // A named key from the row (esc, tab, arrows), with whatever is latched. Locked modifiers are
  // already held down on the computer.
  K.key = (name) => {
    send({ k: "key", key: name, mods: inState("latched") });
    clearLatches();
  };

  function flush() {
    if (!input || composing) return;
    let value = input.value;
    if (!value.startsWith(SENTINEL)) {
      // The sentinel is gone: that deletion was an otherwise-invisible Backspace.
      send({ k: "key", key: "backspace", mods: [] });
      value = SENTINEL + value;
      input.value = value;
    }
    const text = value.slice(1);
    const d = K.diffText(sent, text);
    // A latched or locked modifier makes the next single character a shortcut (Ctrl locked,
    // then "c", is Ctrl+C — the locked key is already held on the computer).
    const latched = inState("latched");
    if ((latched.length || inState("locked").length) && d.back === 0 && cps(d.ins).length === 1) {
      send({ k: "char", ch: d.ins, mods: latched });
      clearLatches();
      input.value = SENTINEL + sent; // it was a shortcut, not text
      return;
    }
    for (let i = 0; i < d.back; i++) send({ k: "key", key: "backspace", mods: [] });
    const lines = d.ins.split("\n");
    lines.forEach((line, i) => {
      if (line) send({ k: "text", text: line });
      if (i < lines.length - 1) send({ k: "key", key: "enter", mods: [] });
    });
    sent = text;
    if (cps(sent).length > WINDOW) { input.value = SENTINEL; sent = ""; }
  }

  // The path changed and the daemon let go of held keys: hold the locked ones again.
  K.resend = () => { if (inState("locked").length) send({ k: "hold", mods: inState("locked") }); };

  // Leaving the app with a modifier locked unlocks it, here and on the computer.
  function hidden() {
    if (document.visibilityState !== "hidden" || !inState("locked").length) return;
    for (const n of NAMES) if (K.mods[n] === "locked") K.mods[n] = "off";
    send({ k: "hold", mods: [] });
    paint();
  }

  function onInput(e) { if (!e.isComposing) flush(); }
  function onStart() { composing = true; }
  function onEnd() { composing = false; flush(); }
  function onKeydown(e) {
    if (e.isComposing) return;
    // Some keyboards never insert "\n", and a keyboard that reports a real Backspace key (unlike
    // Gboard's "Unidentified") on an otherwise-empty field sends no input event for it either.
    if (e.key === "Enter") { e.preventDefault(); flush(); K.key("enter"); }
    else if (e.key === "Backspace" && input.value === SENTINEL) { e.preventDefault(); K.key("backspace"); }
  }

  const on = [["input", onInput], ["compositionstart", onStart], ["compositionend", onEnd], ["keydown", onKeydown]];

  K.attach = (field) => {
    K.detach();
    input = field;
    input.value = SENTINEL;
    sent = "";
    composing = false;
    for (const [ev, fn] of on) input.addEventListener(ev, fn);
    document.addEventListener("visibilitychange", hidden);
  };

  K.detach = () => {
    if (input) for (const [ev, fn] of on) input.removeEventListener(ev, fn);
    document.removeEventListener("visibilitychange", hidden);
    input = null;
    if (inState("locked").length) send({ k: "hold", mods: [] });
    for (const n of NAMES) K.mods[n] = "off";
    paint();
  };
})();
