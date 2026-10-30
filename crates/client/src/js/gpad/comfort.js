// wado bridge — the comfort check (spec §5.8): two thumb-reach fields drawn from the hand pivots
// (landscape: the bottom corners; portrait: low on each side) — green is easy for a resting thumb
// (45 mm), yellow a stretch (62 mm) — converted to px by a per-form estimate (phone 5.6 px/mm,
// tablet 4.7) and a Small / Average / Large hands switch. It is an estimate, never a measurement.
//
// Flags (a breathing warn ring on the control, and a list item that selects it): a stretch, out of
// reach, smaller than 44 px, two controls closer than 8 px. The middle row is never flagged for
// reach; shoulders only when beyond 1.3× the stretch radius. Pull into reach moves flagged controls
// along the line to their pivot until they sit inside green, with the morph.
//
// Tablets: once, when a built-in layout is flagged on first open, "Bring controls closer?" (Q2).
//
//   W.gpadComfort.toggle(on) · draw() · issues() · pull() · offerCloser()

(() => {
  const V = W.gpadVariants, M = W.gpadModel, P = W.gpadPlace;
  const C = (W.gpadComfort = {});
  const G = W.gpadScreen;
  const REACH = { ok: 45, stretch: 62 };
  const HANDS = { small: 0.93, auto: 1, large: 1.05 };

  const tablet = () => Math.min(innerWidth, innerHeight) >= 600;
  C.pxPerMm = () => (tablet() ? 4.7 : 5.6) * (HANDS[G.S.hands] || 1);
  // The two pivots, in play-area px.
  C.pivots = (w, h, o) => (o === "l" ? [[-12, h + 18], [w + 12, h + 18]] : [[-14, h - 30], [w + 14, h - 30]]);

  // Pure on a placement (`gpadPlace.all`), so it is testable without a DOM.
  C.check = (L, place, w, h, o, k) => {
    const out = [];
    const [Lp, Rp] = C.pivots(w, h, o);
    const shown = L.controls.filter((c) => !c.hidden && place.get(c.id));
    const nm = (c) => (c.kind === "btn" ? c.label || V.NAMES[c.bind] : V.NAMES[c.id] || c.label || c.id);
    for (const c of shown) {
      const r = place.get(c.id), cx = r.x + r.w / 2, cy = r.y + r.h / 2;
      const left = cx < w / 2, a = left ? Lp : Rp, side = left ? "left" : "right";
      const d = Math.hypot(cx - a[0], cy - a[1]);
      if (Math.min(r.w, r.h) < V.MIN_PX - 0.5) out.push({ id: c.id, tone: "bad", t: `${nm(c)} is smaller than 44 px` });
      if (c.g === "mid") continue;
      const shoulder = c.g === "sh-l" || c.g === "sh-r";
      if (d > REACH.stretch * k * (shoulder ? 1.3 : 1)) out.push({ id: c.id, tone: "bad", reach: true, t: `${nm(c)} is out of reach for your ${side} thumb` });
      else if (d > REACH.ok * k && !shoulder) out.push({ id: c.id, tone: "warn", reach: true, t: `${nm(c)} is a stretch for your ${side} thumb` });
    }
    for (let i = 0; i < shown.length; i++) for (let j = i + 1; j < shown.length; j++) {
      const a = place.get(shown[i].id), b = place.get(shown[j].id);
      const gx = Math.max(a.x - (b.x + b.w), b.x - (a.x + a.w)), gy = Math.max(a.y - (b.y + b.h), b.y - (a.y + a.h));
      if (Math.max(gx, gy) < 8 && shown[i].kind !== "dpad") out.push({ id: shown[i].id, tone: "warn", t: `${nm(shown[i])} and ${nm(shown[j])} are too close to tell apart` });
    }
    return out;
  };

  const placeNow = (L) => {
    const pa = G.el.pa;
    return P.all(L || G.S.L, G.o(), 1, pa.clientWidth, pa.clientHeight);
  };
  C.issues = () => C.check(G.S.L, placeNow(), G.el.pa.clientWidth, G.el.pa.clientHeight, G.o(), C.pxPerMm());

  C.draw = () => {
    const S = G.S, el = G.el, pa = el.pa;
    const w = pa.clientWidth, h = pa.clientHeight, k = C.pxPerMm(), [Lp, Rp] = C.pivots(w, h, G.o());
    const r = G.paRect();
    const svg = el.comfort.querySelector("svg");
    svg.setAttribute("viewBox", `0 0 ${innerWidth} ${innerHeight}`);
    const ox = r.left, oy = r.top;
    const circ = (a, rad, cls) => `<circle class="${cls}" cx="${a[0] + ox}" cy="${a[1] + oy}" r="${rad}"/>`;
    const arcs = (a) => `<g class="gparcs" style="transform-origin:${a[0] + ox}px ${a[1] + oy}px">${circ(a, REACH.stretch * k, "gpzst")}${circ(a, REACH.ok * k, "gpzok")}</g>`;
    svg.innerHTML = arcs(Lp) + arcs(Rp);
    const issues = C.issues();
    for (const [id, e] of pa._gp || []) e.classList.toggle("flag", issues.some((i) => i.id === id));
    const card = el.ccard;
    const hands = [["small", "Small hands"], ["auto", "Average"], ["large", "Large hands"]];
    const i = Math.max(0, hands.findIndex(([v]) => v === S.hands));
    const pullable = issues.some((x) => x.reach);
    card.innerHTML = `<h4>Comfort check</h4><p class="gpssub">Green is easy for a resting thumb, yellow a stretch. ${tablet() ? "Tablet" : "Phone"} held ${G.o() === "l" ? "sideways" : "upright"} — an estimate, not a measurement.</p>
      <div class="seg" role="radiogroup" aria-label="Hand size" style="--n:3;--i:${i}"><span class="knob"></span>${hands.map(([v, l]) => `<button role="radio" data-v="${v}" aria-checked="${v === S.hands}" class="${v === S.hands ? "on" : ""}">${l}</button>`).join("")}</div>
      <ul class="gpclist">${issues.length ? issues.slice(0, 4).map((x) => `<li><button data-fix="${x.id}"><i class="${x.tone}"></i><span></span></button></li>`).join("") : `<li><button disabled><i class="ok"></i><span>Everything is within easy reach</span></button></li>`}</ul>
      <div class="gprow2">${pullable ? `<button class="btn primary" data-act="pull">Pull into reach</button>` : ""}<button class="btn" data-act="close">Close</button></div>`;
    card.querySelectorAll("[data-fix] span").forEach((s, j) => { s.textContent = issues[j].t; });
    card.querySelector(".seg").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-v]");
      if (!b) return;
      e.currentTarget.style.setProperty("--i", hands.findIndex(([v]) => v === b.dataset.v));
      setTimeout(() => { S.hands = b.dataset.v; C.draw(); }, 160);
    });
    card.querySelectorAll("[data-fix]").forEach((b) => b.addEventListener("click", () => {
      C.toggle(false);
      S.sel = b.dataset.fix;
      S.tab = "place";
      G.draw();
      W.gpadInspector.draw();
    }));
    const pull = card.querySelector('[data-act="pull"]');
    if (pull) pull.addEventListener("click", C.pull);
    card.querySelector('[data-act="close"]').addEventListener("click", () => C.toggle(false));
  };

  // Each flagged control slides along the line to its pivot until it sits inside green.
  C.pull = () => {
    const S = G.S, pa = G.el.pa, o = G.o(), [DW, DH] = V.REF[o];
    const w = pa.clientWidth, h = pa.clientHeight, k = C.pxPerMm(), [Lp, Rp] = C.pivots(w, h, o);
    G.snapshot();
    const next = JSON.parse(JSON.stringify(S.L));
    const place = placeNow(next);
    for (const c of next.controls) {
      if (c.hidden || c.g === "mid" || c.g === "sh-l" || c.g === "sh-r") continue;
      const r = place.get(c.id);
      if (!r) continue;
      const cx = r.x + r.w / 2, cy = r.y + r.h / 2, a = cx < w / 2 ? Lp : Rp;
      const d = Math.hypot(cx - a[0], cy - a[1]), lim = REACH.ok * k * 0.97;
      if (d <= lim) continue;
      const f = lim / d;
      c[o][0] += (a[0] + (cx - a[0]) * f - cx) / DW;
      c[o][1] += (a[1] + (cy - a[1]) * f - cy) / DH;
      M.clampPos(c, o);
    }
    G.morphTo(next);
    G.commit();
    G.say("Pulled into reach");
    setTimeout(() => { if (S.comfort) C.draw(); }, W.gpadMotion.ms("--dur-hero", 520) + 60);
  };

  C.toggle = (on) => {
    const S = G.S, el = G.el;
    if (!S) return;
    S.comfort = !!on;
    el.comf.setAttribute("aria-pressed", String(S.comfort));
    el.comfort.classList.toggle("on", S.comfort);
    el.ccard.classList.toggle("on", S.comfort);
    W.gpadInspector.draw();
    if (S.comfort) C.draw();
    else for (const e of (el.pa._gp || new Map()).values()) e.classList.remove("flag");
  };

  // A tablet opening a built-in layout whose controls sit out of reach: offered once.
  C.offerCloser = () => {
    const S = G.S;
    if (!tablet() || S.editing || S.L.id.startsWith("mine-")) return;
    try { if (localStorage.getItem("wado.gpadCloser")) return; } catch (_) { return; }
    if (!C.issues().some((x) => x.reach)) return;
    try { localStorage.setItem("wado.gpadCloser", "1"); } catch (_) {}
    const card = G.el.ccard;
    card.innerHTML = `<h4>Bring controls closer?</h4><p class="gpssub">On a screen this big some of ${S.L.name}'s controls sit beyond a resting thumb. wado can move them into reach and save it as your own layout.</p>
      <div class="gprow2"><button class="btn primary" data-act="yes">Bring closer</button><button class="btn" data-act="no">Not now</button></div>`;
    card.classList.add("on");
    card.querySelector('[data-act="no"]').onclick = () => card.classList.remove("on");
    card.querySelector('[data-act="yes"]').onclick = () => { card.classList.remove("on"); C.pull(); G.flush(); };
  };
})();
