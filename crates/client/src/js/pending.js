// wado bridge — every action shows its state (Decision Log 2026-10-30): one primitive for every
// button that asks the computer to do something.
//
// Four stages, keyed by an action id, resolved by the action's answer on the wire:
//
//   pressed   at once: the control scales under the finger (`data-press`, any button) and, for
//             an action, the phone ticks where it can
//   working   after WORK_MS with no outcome: progress in place (a ring, the label "Closing…"),
//             `aria-busy`, and further taps are swallowed — no double submits
//   long      past the action's `longMs`: it says what it is waiting for ("Still waiting for
//             bushido to answer…") on a card with Cancel and Try again
//   outcome   ok: a check morph in place, and a card only when the result lands off-screen;
//             failed: why, in plain words, and Try again; no answer by `timeoutMs`: "bushido
//             didn't answer", and Try again — never a frozen button
//
//   W.pending.start(spec)   → {id, ack}, or false if that action is already under way (then the
//                             caller sends nothing: no double submits). `ack` is the number to
//                             put on the request; the daemon echoes it on its answer
//     spec = { id, label, what, wait, longMs, timeoutMs, ok, fail, accept, acceptLabel, done,
//              quiet, check, el, retry, cancel, rust }
//       ok / fail / accept   wire matchers: "type", "type?key=value", or fn(type, msg) → truthy
//                            (fail's truthy string is the reason). `$ack` in a matcher is this
//                            action's ack; `ack=` also matches an answer that carries none (an
//                            older daemon answers in order). A string matcher *consumes*
//                            the message (FIFO across actions: two launches, two answers); a
//                            function only observes state and consumes nothing.
//       el                   the control(s); otherwise every `[data-act="<id>"]`, otherwise the
//                            control pressed in the last second
//       retry / cancel       functions; or `rust: true` to send {type:"pendingRetry"|"pendingCancel", id}
//       fresh                the answer is one only a daemon with acks sends (`session_detached`,
//                            `config_saved`, `clipboard_sent`): an older one is taken at its word
//   W.pending.wire(type, msg)   every message from the computer passes here (relay_link.js `fire`,
//                               and the few synthetic answers the bridge makes)
//   W.pending.ok(id, text) / fail(id, why) / accept(id, label) / cancel(id) / retry(id)
//   W.pending.state(id)         "press" | "work" | "long" | "accepted" | "ok" | "fail" | "silent" | ""

(() => {
  const P = (W.pending = {});
  const WORK_MS = 300;
  const acts = new Map();
  let seq = 0, ackSeq = 0;
  // A daemon that echoes acks has the answers older ones never sent (set on the first echo).
  W.daemonAcks = false;
  P.nextAck = () => ++ackSeq;
  const root = document.documentElement;

  const motion = () => {
    const m = root.dataset.motion;
    if (m === "off" || m === "reduced") return m;
    try { if (matchMedia("(prefers-reduced-motion: reduce)").matches) return "reduced"; } catch (_) {}
    return "full";
  };

  // ── the hand: a press state on every control, at pointerdown ───────────────────────────────
  // `:active` comes late or not at all on a touchscreen (Chrome sets it after its tap detection,
  // iOS only with a touch listener), so a finger saw nothing for 110–250 ms (measured,
  // scripts/console-perf.mjs). `data-press` is set in the pointerdown itself.
  const PRESSABLE = "button, [role='button'], [data-act], .tile, .tpact, .tpkey";
  let pressed = null;
  let lastPress = { el: null, t: 0, touch: false, buzzed: false };
  const haptic = (kind) => {
    try {
      if (W.native && W.native.haptic) { W.native.haptic(kind); return; }
      if (navigator.vibrate) navigator.vibrate(kind === "reject" ? [12, 60, 12] : kind === "confirm" ? 10 : 6);
    } catch (_) {}
  };
  P.haptic = haptic;
  const release = () => { if (pressed) { pressed.removeAttribute("data-press"); pressed = null; } };
  addEventListener("pointerdown", (e) => {
    const el = e.target && e.target.closest && e.target.closest(PRESSABLE);
    release();
    if (!el || el.disabled || el.closest("[inert]")) return;
    pressed = el;
    el.setAttribute("data-press", "");
    const touch = e.pointerType === "touch" || e.pointerType === "pen";
    lastPress = { el, t: performance.now(), touch, buzzed: false };
    // An action ticks under the finger now, not when its handler gets round to it.
    if (touch && el.hasAttribute("data-act") && !busyEl(el)) { haptic("tick"); lastPress.buzzed = true; }
  }, { capture: true, passive: true });
  for (const ev of ["pointerup", "pointercancel", "dragstart"]) addEventListener(ev, release, { capture: true, passive: true });
  // A finger that slides off the control lets go of it, as a native button does.
  addEventListener("pointerout", (e) => { if (pressed && e.target === pressed && !pressed.contains(e.relatedTarget)) release(); }, { capture: true, passive: true });

  // Working: no second submit. Capture phase, so neither Dioxus nor a JS handler sees the tap.
  const busyEl = (el) => { const s = el.getAttribute("data-pstate"); return s === "work" || s === "long" || s === "accepted"; };
  addEventListener("click", (e) => {
    const el = e.target && e.target.closest && e.target.closest("[data-pstate]");
    if (el && busyEl(el)) { e.preventDefault(); e.stopImmediatePropagation(); }
  }, true);

  // ── the voice: one polite live region, and the cards ───────────────────────────────────────
  let said = null, deck = null;
  function say(text) {
    if (!said || !said.isConnected) {
      said = document.createElement("div");
      said.id = "pending-said";
      said.className = "sr-only";
      said.setAttribute("role", "status");
      said.setAttribute("aria-live", "polite");
      document.body.appendChild(said);
    }
    // The same words twice are still news (a second failure): clear, then set.
    said.textContent = "";
    setTimeout(() => { said.textContent = text; }, 30);
  }
  function cards() {
    if (!deck || !deck.isConnected) {
      deck = document.createElement("div");
      deck.id = "pending-cards";
      document.body.appendChild(deck);
    }
    return deck;
  }

  const ICON = {
    check: "M20 6 9 17l-5-5",
    alert: "M12 9v4M12 17h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z",
    x: "M18 6 6 18M6 6l12 12",
  };
  const svg = (name) => `<svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="${ICON[name]}"/></svg>`;
  const RING = `<svg class="pnd-ring" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/></svg>`;
  const esc = (s) => String(s == null ? "" : s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

  // One card per action: replaced in place as the stage moves, removed on dismiss or a quiet end.
  function card(a, tone, text, buttons, ttl) {
    const host = cards();
    let c = a.card;
    if (!c || !c.isConnected) {
      c = document.createElement("div");
      c.className = "pcard";
      c.setAttribute("role", "group");
      c.dataset.pid = a.id;
      host.appendChild(c);
      a.card = c;
      // At most three: the oldest finished one goes first.
      const all = [...host.children];
      if (all.length > 3) (all.find((x) => x.dataset.tone !== "work") || all[0]).remove();
    }
    c.dataset.tone = tone;
    const glyph = tone === "work" ? RING : tone === "ok" ? svg("check") : svg("alert");
    c.innerHTML = `<span class="pcg">${glyph}</span><span class="pct">${esc(text)}</span>` +
      buttons.map((b) => `<button type="button" class="pcb${b.primary ? " primary" : ""}" data-pcb="${b.k}">${esc(b.label)}</button>`).join("") +
      (tone === "work" ? "" : `<button type="button" class="pcx" data-pcb="dismiss" aria-label="Dismiss">${svg("x")}</button>`);
    clearTimeout(a.cardTimer);
    if (ttl) a.cardTimer = setTimeout(() => dropCard(a), ttl);
  }
  function dropCard(a) {
    const c = a.card;
    a.card = null;
    if (!c || !c.isConnected) return;
    if (motion() === "off") { c.remove(); return; }
    c.classList.add("out");
    setTimeout(() => c.remove(), 200);
  }
  addEventListener("click", (e) => {
    const b = e.target && e.target.closest && e.target.closest("#pending-cards [data-pcb]");
    if (!b) return;
    const id = b.closest(".pcard").dataset.pid;
    const a = acts.get(id);
    const k = b.dataset.pcb;
    if (k === "retry") P.retry(id);
    else if (k === "cancel") P.cancel(id);
    else if (a) { dropCard(a); if (isEnd(a.stage)) acts.delete(id); }
    else b.closest(".pcard").remove();
  });

  // ── the control: its state, in place ─────────────────────────────────────────────────────
  function controls(a) {
    const set = new Set();
    const add = (x) => { if (x && x.nodeType === 1 && x.isConnected) set.add(x); };
    if (a.spec.el) {
      if (typeof a.spec.el === "string") document.querySelectorAll(a.spec.el).forEach(add);
      else add(a.spec.el);
    }
    try { document.querySelectorAll(`[data-act="${CSS.escape(a.id)}"]`).forEach(add); } catch (_) {}
    if (!set.size && a.pressEl) add(a.pressEl);
    return [...set];
  }
  const visible = (el) => {
    if (!el.isConnected || el.closest("[inert]")) return false;
    const r = el.getBoundingClientRect();
    if (!r.width || !r.height || r.bottom < 0 || r.top > innerHeight || r.right < 0 || r.left > innerWidth) return false;
    const s = getComputedStyle(el);
    return s.visibility !== "hidden" && +s.opacity > 0.05;
  };
  const onScreen = (a) => controls(a).some(visible);

  function paint(a) {
    const els = controls(a);
    for (const el of els) dress(el, a);
    // Controls that went (a page changed, a list re-rendered) are let go.
    for (const el of a.dressed) if (!els.includes(el)) undress(el);
    a.dressed = els;
  }
  function dress(el, a) {
    const st = a.stage;
    el.setAttribute("data-pstate", st);
    const busy = st === "work" || st === "long" || st === "accepted";
    if (busy) { el.setAttribute("aria-busy", "true"); el.setAttribute("aria-disabled", "true"); }
    else { el.removeAttribute("aria-busy"); el.removeAttribute("aria-disabled"); }
    let o = el.querySelector(":scope > .pnd");
    const showing = busy || st === "ok";
    if (!showing) { if (o) o.remove(); return; }
    if (!o) {
      // The control's own ink, taken before it is hidden.
      const ink = getComputedStyle(el).color;
      o = document.createElement("span");
      o.className = "pnd";
      o.setAttribute("aria-hidden", "true");
      o.style.color = ink;
      el.appendChild(o);
      if (getComputedStyle(el).position === "static") el.setAttribute("data-prel", "");
    }
    // A control too narrow for words keeps to the ring; the card and the voice carry them.
    const wide = el.getBoundingClientRect().width >= 88;
    const words = st === "ok" ? (a.spec.doneShort || "") : st === "long" ? "Still waiting…" : st === "accepted" ? (a.acceptLabel || a.spec.label) : a.spec.label;
    const key = st + "|" + (wide ? words : "");
    if (o.dataset.k === key) return;
    o.dataset.k = key;
    o.innerHTML = (st === "ok" ? `<span class="pnd-ok">${svg("check")}</span>` : RING) +
      (wide && words ? `<span class="pnd-t">${esc(words)}</span>` : "");
  }
  function undress(el) {
    el.removeAttribute("data-pstate");
    el.removeAttribute("aria-busy");
    el.removeAttribute("aria-disabled");
    el.removeAttribute("data-prel");
    const o = el.querySelector(":scope > .pnd");
    if (o) o.remove();
  }

  // A list that re-renders under a pending action (home's Running, a page) gets its state back.
  let mo = null;
  function watch() {
    const any = [...acts.values()].some((a) => !isEnd(a.stage) || a.stage === "ok");
    if (any && !mo && typeof MutationObserver !== "undefined") {
      let queued = false;
      mo = new MutationObserver(() => {
        if (queued) return;
        queued = true;
        requestAnimationFrame(() => { queued = false; for (const a of acts.values()) if (a.stage !== "press") paint(a); });
      });
      mo.observe(document.body, { childList: true, subtree: true });
    } else if (!any && mo) { mo.disconnect(); mo = null; }
  }

  // ── the stages ────────────────────────────────────────────────────────────────────────────
  const isEnd = (s) => s === "ok" || s === "fail" || s === "silent" || s === "gone";
  const waitFor = (a) => a.spec.wait || W.computerName || "your computer";

  function to(a, stage) {
    a.stage = stage;
    if (W.fixture && W.fixtureLog) W.fixtureLog.push([a.id, stage, Math.round(performance.now() - a.t0)]);
    paint(a);
    watch();
  }

  function clear(a) { for (const t of a.timers) clearTimeout(t); a.timers = []; }

  P.start = (spec) => {
    if (!spec || !spec.id) return false;
    const old = acts.get(spec.id);
    if (old && !isEnd(old.stage)) return false; // under way: a double submit
    if (old) { clear(old); dropCard(old); for (const el of old.dressed) undress(el); }
    const now = performance.now();
    const fresh = lastPress.el && now - lastPress.t < 1000;
    const ack = ++ackSeq;
    const sub = (m) => Array.isArray(m) ? m.map(sub) : typeof m === "string" ? m.replace(/\$ack\b/g, String(ack)) : m;
    spec = { ...spec, ok: sub(spec.ok), fail: sub(spec.fail), accept: sub(spec.accept) };
    const a = { id: spec.id, spec, n: ++seq, ack, t0: now, stage: "press", timers: [], dressed: [], card: null,
      pressEl: fresh ? lastPress.el : null, acceptLabel: "" };
    acts.set(a.id, a);
    if (fresh && lastPress.touch && !lastPress.buzzed) haptic("tick");
    const longMs = spec.longMs || 4000, timeoutMs = Math.max(spec.timeoutMs || 15000, longMs + 1000);
    a.timers.push(setTimeout(() => {
      if (a.stage !== "press") return;
      to(a, "work");
      // Nothing on screen to show it (the session closed, the sheet went): the card carries it.
      if (!onScreen(a)) card(a, "work", a.spec.label, a.spec.cancel || a.spec.rust ? [{ k: "cancel", label: "Cancel" }] : []);
    }, spec.workMs || WORK_MS));
    a.timers.push(setTimeout(() => long(a), longMs));
    a.timers.push(setTimeout(() => silent(a), timeoutMs));
    paint(a);
    // An older daemon never sends this answer: the request going out is all there is to know.
    if (spec.fresh && !W.daemonAcks) setTimeout(() => P.ok(a.id), 0);
    return { id: a.id, ack };
  };

  function long(a) {
    if (isEnd(a.stage)) return;
    to(a, a.stage === "accepted" ? "accepted" : "long");
    const text = a.stage === "accepted" && a.spec.acceptLong ? a.spec.acceptLong : a.spec.long || `Still waiting for ${waitFor(a)} to answer…`;
    card(a, "work", text, [{ k: "cancel", label: "Cancel" }, { k: "retry", label: "Try again", primary: true }]);
    say(text);
  }

  function silent(a) {
    if (isEnd(a.stage)) return;
    // Accepted, then nothing visible (an app that is slow to open a window): not an error.
    if (a.stage === "accepted" && a.spec.acceptedOk) return P.ok(a.id, a.spec.acceptedOk);
    clear(a);
    to(a, "silent");
    const what = a.spec.what ? ` ${a.spec.what} may not have happened.` : "";
    const text = `${cap(waitFor(a))} didn't answer.${what}`;
    card(a, "bad", text, [{ k: "retry", label: "Try again", primary: true }]);
    say(text);
    haptic("reject");
    settle(a, 2400);
  }

  P.ok = (id, text) => {
    const a = acts.get(id);
    if (!a || isEnd(a.stage)) return;
    clear(a);
    const quick = a.stage === "press";
    const seen = onScreen(a);
    to(a, quick && a.spec.check === false ? "gone" : "ok");
    const done = text || a.spec.done || "";
    if (done) say(done);
    if (a.stage === "ok" && motion() !== "off" && a.dressed.length) haptic("confirm");
    // Off-screen, the card says it; on screen, the check does, quietly.
    if (done && !a.spec.quiet && !seen) card(a, "ok", done, [], 2600);
    else dropCard(a);
    settle(a, a.stage === "ok" ? 900 : 0);
  };

  P.fail = (id, why) => {
    const a = acts.get(id);
    if (!a || isEnd(a.stage)) return;
    clear(a);
    to(a, "fail");
    const plain = String(why || "").trim() || "It did not work.";
    const text = a.spec.what ? `${a.spec.what} didn't work: ${plain}` : plain;
    card(a, "bad", text, a.spec.retry || a.spec.rust ? [{ k: "retry", label: "Try again", primary: true }] : []);
    say(text);
    haptic("reject");
    settle(a, 2400);
  };

  P.accept = (id, label) => {
    const a = acts.get(id);
    if (!a || isEnd(a.stage) || a.stage === "accepted") return;
    a.acceptLabel = label || a.spec.acceptLabel || a.spec.label;
    const was = a.stage;
    to(a, "accepted");
    // The computer took it: the wait from here is the app, so the clock restarts.
    clear(a);
    const longMs = a.spec.acceptLongMs || 6000, timeoutMs = a.spec.acceptTimeoutMs || 20000;
    if (was === "press") a.timers.push(setTimeout(() => paint(a), 0));
    a.timers.push(setTimeout(() => long(a), longMs));
    a.timers.push(setTimeout(() => silent(a), timeoutMs));
    if (a.card) card(a, "work", a.acceptLabel, [{ k: "cancel", label: "Cancel" }]);
  };

  P.cancel = (id) => {
    const a = acts.get(id);
    if (!a) return;
    clear(a);
    const supported = !!(a.spec.cancel || a.spec.rust);
    try { if (a.spec.cancel) a.spec.cancel(); else if (a.spec.rust) emit({ type: "pendingCancel", id }); } catch (_) {}
    to(a, "gone");
    dropCard(a);
    say(supported ? "Cancelled." : `Stopped waiting. It may still happen on ${waitFor(a)}.`);
    settle(a, 0);
  };

  P.retry = (id) => {
    const a = acts.get(id);
    if (!a) return;
    clear(a);
    dropCard(a);
    to(a, "gone");
    for (const el of a.dressed) undress(el);
    acts.delete(id);
    try {
      if (typeof a.spec.retry === "function") a.spec.retry();
      else if (a.spec.rust) emit({ type: "pendingRetry", id });
    } catch (_) {}
  };

  // The control goes back to itself after the outcome has been seen.
  function settle(a, ms) {
    a.timers.push(setTimeout(() => {
      for (const el of a.dressed) undress(el);
      a.dressed = [];
      if (!a.card) acts.delete(a.id);
      watch();
    }, ms));
  }

  P.state = (id) => { const a = acts.get(id); return a ? a.stage : ""; };
  P.busy = (id) => { const a = acts.get(id); return !!a && !isEnd(a.stage); };

  // ── answers ───────────────────────────────────────────────────────────────────────────────
  // "type" or "type?key=value" (several keys with &): a string matcher, which consumes.
  function match(m, type, msg) {
    if (!m) return false;
    if (Array.isArray(m)) { for (const x of m) { const r = match(x, type, msg); if (r) return r; } return false; }
    if (typeof m === "function") return m(type, msg);
    const [t, q] = String(m).split("?");
    if (t !== type) return false;
    if (q) for (const kv of q.split("&")) {
      const [k, v] = kv.split("=");
      const got = msg && msg[k];
      // An answer without an ack is from a daemon that answers in order: FIFO takes it.
      if (k === "ack" && !got) continue;
      if (String(got) !== v) return false;
    }
    return true;
  }
  const consumes = (m, type, msg) => Array.isArray(m) ? m.some((x) => typeof x === "string" && match(x, type, msg)) : typeof m === "string";

  P.wire = (type, msg) => {
    if (msg && msg.ack > 0) W.daemonAcks = true;
    if (!acts.size) return;
    const open = [...acts.values()].filter((a) => !isEnd(a.stage)).sort((x, y) => x.n - y.n);
    let used = { ok: false, fail: false, accept: false };
    for (const a of open) {
      const s = a.spec;
      if (s.fail && !(used.fail && consumes(s.fail, type, msg))) {
        const r = match(s.fail, type, msg);
        if (r) {
          if (consumes(s.fail, type, msg)) used.fail = true;
          P.fail(a.id, typeof r === "string" ? r : (msg && (msg.message || msg.error)) || "");
          continue;
        }
      }
      if (s.ok && !(used.ok && consumes(s.ok, type, msg))) {
        const r = match(s.ok, type, msg);
        if (r) { if (consumes(s.ok, type, msg)) used.ok = true; P.ok(a.id, typeof r === "string" ? r : undefined); continue; }
      }
      if (s.accept && a.stage !== "accepted" && !(used.accept && consumes(s.accept, type, msg))) {
        const r = match(s.accept, type, msg);
        if (r) { if (consumes(s.accept, type, msg)) used.accept = true; P.accept(a.id); }
      }
    }
  };

  const cap = (s) => String(s).charAt(0).toUpperCase() + String(s).slice(1);

  // Tests (`?fixture` only): a record of every stage, with its time since the press.
  if (W.fixture) W.fixtureLog = [];
})();
