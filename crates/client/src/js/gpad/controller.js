// wado bridge — one controller's state: what is held and where the axes are, between the pad's
// input (`gpad/input.js`) and whichever wire carries it (`gpad/send_session.js` in a session,
// `gpad/send_pad.js` on the pad link). Only changes go on: a thumb resting at full deflection is
// one axis value, not sixty a second. It is also what the "Framework sees" readout reads, so the
// person sees what actually left the phone (§5.5).
//
// Binds: a b x y lb rb lt rt back start guide l3 r3 (buttons; lt/rt digital), du dd dl dr (the
// hat, as four buttons), and the axes lx ly rx ry (−1..1, y down) and lt rt (0..1, a pedal).
//
//   const c = W.gpadController(sender)   sender: {button(bind, down), axes(a, changed), release()}
//   c.button(bind, down) · c.stick("ls"|"rs", x, y) · c.trigger("lt"|"rt", v) · c.releaseAll()
//   c.sees() → ["A", "D-pad ↑", "LT 70%", …]   c.axes → {lx, ly, rx, ry, lt, rt}
//   c.onChange(fn) · c.setSender(sender)

(() => {
  const NAME = {
    a: "A", b: "B", x: "X", y: "Y", lb: "LB", rb: "RB", lt: "LT", rt: "RT",
    back: "Back", start: "Start", guide: "Guide", l3: "L3", r3: "R3",
  };
  const HAT = { du: "↑", dd: "↓", dl: "←", dr: "→" };

  W.gpadController = (sender) => {
    const held = new Set();
    const axes = { lx: 0, ly: 0, rx: 0, ry: 0, lt: 0, rt: 0 };
    const fns = [];
    let out = sender || null;
    const changed = () => { for (const fn of fns) { try { fn(); } catch (_) {} } };

    const c = {
      held, axes,
      setSender(s) { out = s; },
      onChange(fn) { fns.push(fn); },
      button(bind, down) {
        if (held.has(bind) === !!down) return;
        if (down) held.add(bind); else held.delete(bind);
        if (out) out.button(bind, !!down);
        changed();
      },
      setAxes(next) {
        const diff = [];
        for (const k in next) {
          const v = Math.round(next[k] * 1000) / 1000;
          if (axes[k] !== v) { axes[k] = v; diff.push(k); }
        }
        if (!diff.length) return;
        if (out) out.axes(axes, diff);
        changed();
      },
      stick(bind, x, y) { c.setAxes(bind === "rs" ? { rx: x, ry: y } : { lx: x, ly: y }); },
      trigger(bind, v) { c.setAxes({ [bind]: Math.max(0, Math.min(1, v)) }); },
      releaseAll() {
        for (const b of [...held]) c.button(b, false);
        c.setAxes({ lx: 0, ly: 0, rx: 0, ry: 0, lt: 0, rt: 0 });
        if (out && out.release) out.release();
      },
      offCentre: () => Object.values(axes).some((v) => v !== 0),
      sees() {
        const out = [];
        const hat = Object.keys(HAT).filter((d) => held.has(d)).map((d) => HAT[d]).join("");
        if (hat) out.push(`D-pad ${hat}`);
        for (const b of held) if (NAME[b] && !(axes[b] && (b === "lt" || b === "rt"))) out.push(NAME[b]);
        for (const t of ["lt", "rt"]) if (axes[t] > 0) out.push(`${NAME[t]} ${Math.round(axes[t] * 100)}%`);
        for (const [s, x, y] of [["L stick", axes.lx, axes.ly], ["R stick", axes.rx, axes.ry]]) {
          const m = Math.hypot(x, y);
          if (m > 0) out.push(`${s} ${Math.round(Math.min(1, m) * 100)}%`);
        }
        return out;
      },
    };
    return c;
  };
})();
