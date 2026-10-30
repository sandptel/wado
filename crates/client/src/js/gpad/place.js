// wado bridge — where a layout's controls land on a screen (spec §9.3). Pure: boxes are numbers.
//
// Positions are designed on a reference phone (`REF`) and re-anchored on any other box: a group in
// the left third keeps its px distance from the left edge, the right third from the right, the
// middle third from the centre (the same for rows). Members keep their px offsets from the group's
// centre. So on a tablet the sticks stay under the thumbs and a cluster never spreads apart; only
// the empty middle grows (D8). `k` scales everything: 1 on the play screen, box/REF for a thumbnail.
//
// The Pocket Console's deck is a box too small for a phone layout (§4): `deck()` packs each side's
// groups into its own part of the box — left, centre, right (or, in a landscape console, the
// gutters beside the picture and the band under it) — keeping each group's shape, size and look,
// scaled down only as far as the 44 px floor allows. A group that will not fit even then is left
// out of the deck, most important kept first (`PRIORITY`); full screen always has them all.
//
//   W.gpadPlace.orient(w, h)                       "l" | "p"
//   W.gpadPlace.at(c, L, o, k, w, h)               → [cx, cy] in box px
//   W.gpadPlace.size(c, o, k)                      → [w, h] in px
//   W.gpadPlace.all(L, o, k, w, h)                 → Map id → {x, y, w, h} (top-left box)
//   W.gpadPlace.deck(L, w, h, pic?)                → the same, packed for a console deck
//   W.gpadPlace.thumbK(o, w, h)                    the k that fits a whole layout in w × h

(() => {
  const V = W.gpadVariants;
  const P = (W.gpadPlace = {});

  P.orient = (w, h) => (w >= h ? "l" : "p");
  P.anchor = (f, D, A, k) => (f < 1 / 3 ? f * D * k : f > 2 / 3 ? A - (1 - f) * D * k : A / 2 + (f - 0.5) * D * k);

  P.groupCentre = (L, g, o) => {
    const ms = L.controls.filter((x) => x.g === g && !x.hidden).map((x) => x[o]);
    if (!ms.length) return [0.5, 0.5];
    return [ms.reduce((a, p) => a + p[0], 0) / ms.length, ms.reduce((a, p) => a + p[1], 0) / ms.length];
  };

  P.size = (c, o, k) => {
    const [w, h] = V.BASE[c.kind] || V.BASE.btn;
    const s = c[o][2] || 1;
    return [w * s * k, h * s * k];
  };

  P.at = (c, L, o, k, A, B) => {
    const [DW, DH] = V.REF[o], pos = c[o];
    const gc = P.groupCentre(L, c.g, o);
    return [P.anchor(gc[0], DW, A, k) + (pos[0] - gc[0]) * DW * k, P.anchor(gc[1], DH, B, k) + (pos[1] - gc[1]) * DH * k];
  };

  P.all = (L, o, k, A, B) => {
    const out = new Map();
    for (const c of L.controls) {
      if (c.hidden) continue;
      const [cx, cy] = P.at(c, L, o, k, A, B);
      const [w, h] = P.size(c, o, k);
      out.set(c.id, { x: cx - w / 2, y: cy - h / 2, w, h });
    }
    return out;
  };

  P.thumbK = (o, w, h) => Math.min(w / V.REF[o][0], h / V.REF[o][1]);

  // The smallest k at which `c` is still 44 px on its short side.
  const floorK = (c, o) => {
    const [w, h] = P.size(c, o, 1);
    return V.MIN_PX / Math.min(w, h);
  };

  // Bounding box of controls in reference px (orientation `o`).
  function bbox(cs, o) {
    const [DW, DH] = V.REF[o];
    let l = 1e9, t = 1e9, r = -1e9, b = -1e9;
    for (const c of cs) {
      const [w, h] = P.size(c, o, 1);
      const x = c[o][0] * DW, y = c[o][1] * DH;
      l = Math.min(l, x - w / 2); r = Math.max(r, x + w / 2);
      t = Math.min(t, y - h / 2); b = Math.max(b, y + h / 2);
    }
    return { l, t, w: Math.max(1, r - l), h: Math.max(1, b - t) };
  }

  // Lay `cs` out with their bbox's top-left at (x0, y0): offsets at scale `kp`; each control as
  // small as `kp` but never under its own 44 px floor; each kept inside the A × B box.
  function lay(out, cs, o, kp, bb, x0, y0, A, B, X = 0, Y = 0) {
    const [DW, DH] = V.REF[o];
    for (const c of cs) {
      const [w, h] = P.size(c, o, Math.max(kp, floorK(c, o)));
      const cx = x0 + (c[o][0] * DW - bb.l) * kp, cy = y0 + (c[o][1] * DH - bb.t) * kp;
      // Kept inside [X, A] × [Y, B].
      const x = Math.min(Math.max(X, cx - w / 2), Math.max(X, A - w));
      const y = Math.min(Math.max(Y, cy - h / 2), Math.max(Y, B - h));
      out.set(c.id, { x, y, w, h });
    }
  }

  P.side = (L, c, o) => {
    const gx = P.groupCentre(L, c.g, o)[0];
    return gx < 1 / 3 ? "left" : gx > 2 / 3 ? "right" : "centre";
  };

  // What a small deck keeps first: the thumbs' own controls, then the rest.
  P.PRIORITY = ["dpad", "face", "stick-l", "stick-r", "steer", "pedals", "sh-l", "sh-r", "mid"];
  const rank = (g) => { const i = P.PRIORITY.indexOf(g); return i < 0 ? P.PRIORITY.length : i; };

  // The groups of `cs`, most important first, each as {cs, bb}: its controls and their bbox at 1×.
  function groups(cs, o) {
    const by = new Map();
    for (const c of cs) { if (!by.has(c.g)) by.set(c.g, []); by.get(c.g).push(c); }
    return [...by.entries()].sort((x, y) => rank(x[0]) - rank(y[0])).map(([g, m]) => ({ g, cs: m, bb: bbox(m, o) }));
  }

  const round = (c) => c.kind === "stick" || c.kind === "dpad" || (c.kind === "btn" && c.look?.shape !== "pill" && c.look?.shape !== "round");
  // How far a group's offsets can shrink before two members touch, each at its 44 px floor: round
  // controls by the distance between centres, the rest by their boxes.
  function closest(G, o) {
    const [DW, DH] = V.REF[o];
    let k = 0;
    for (let i = 0; i < G.cs.length; i++) for (let j = i + 1; j < G.cs.length; j++) {
      const a = G.cs[i], b = G.cs[j];
      const [aw, ah] = P.size(a, o, floorK(a, o)), [bw, bh] = P.size(b, o, floorK(b, o));
      const dx = Math.abs(a[o][0] - b[o][0]) * DW, dy = Math.abs(a[o][1] - b[o][1]) * DH;
      const need = round(a) && round(b)
        ? (Math.min(aw, ah) + Math.min(bw, bh)) / 2 / Math.max(1e-6, Math.hypot(dx, dy))
        : Math.min(dx ? (aw + bw) / 2 / dx : Infinity, dy ? (ah + bh) / 2 / dy : Infinity);
      k = Math.max(k, need);
    }
    return Math.min(1, k);
  }
  // A group's footprint with its offsets at `k` and each member at max(k, its floor).
  function footprint(G, o, k) {
    const [DW, DH] = V.REF[o];
    let l = 1e9, t = 1e9, r = -1e9, b = -1e9;
    for (const c of G.cs) {
      const [w, h] = P.size(c, o, Math.max(k, floorK(c, o)));
      const x = (c[o][0] * DW - G.bb.l) * k, y = (c[o][1] * DH - G.bb.t) * k;
      l = Math.min(l, x - w / 2); r = Math.max(r, x + w / 2); t = Math.min(t, y - h / 2); b = Math.max(b, y + h / 2);
    }
    return { l, t, w: r - l, h: b - t };
  }
  // The largest k (≤ 1, ≥ the group's own closest) whose footprint fits w × h, or null.
  function fit(G, o, w, h) {
    const lo = closest(G, o);
    const ok = (k) => { const f = footprint(G, o, k); return f.w <= w + 0.5 && f.h <= h + 0.5; };
    if (!ok(lo)) return null;
    if (ok(1)) return 1;
    let a = lo, b = 1;
    for (let i = 0; i < 18; i++) { const m = (a + b) / 2; if (ok(m)) a = m; else b = m; }
    return a;
  }

  // Flow a side's groups into its region, most important first: a row in a wide region, a column
  // in a tall one; each group as large as fits, its members never under 44 px and never on top of
  // each other; a group that will not fit is left out of this deck (full screen has it all).
  function flow(out, gs, o, r, gap) {
    const wide = r.w >= r.h;
    const placed = [];
    let used = 0;
    for (const G of gs) {
      const room = (wide ? r.w : r.h) - used;
      const k = fit(G, o, wide ? room : r.w, wide ? r.h : room);
      if (k == null) continue;
      const f = footprint(G, o, k);
      placed.push({ G, k, f, at: used });
      used += (wide ? f.w : f.h) + gap;
    }
    used -= gap;
    const lead = Math.max(0, ((wide ? r.w : r.h) - used) / 2);
    for (const p of placed) {
      const x0 = r.x + (wide ? lead + p.at : (r.w - p.f.w) / 2) - p.f.l, y0 = r.y + (wide ? (r.h - p.f.h) / 2 : lead + p.at) - p.f.t;
      lay(out, p.G.cs, o, p.k, p.G.bb, x0, y0, r.x + r.w, r.y + r.h, r.x, r.y);
    }
  }

  // `pic`: the picture's rect inside the box, when the deck surrounds it (the landscape console,
  // where the pad's slot is the gutters beside the picture and the band under it).
  P.deck = (L, A, B, pic, pad = 6) => {
    const out = new Map();
    const shown = L.controls.filter((c) => !c.hidden);
    if (!shown.length) return out;
    const sides = { left: [], centre: [], right: [] };
    for (const c of shown) sides[P.side(L, c, "l")].push(c);
    const gs = {};
    for (const s in sides) gs[s] = groups(sides[s], "l");
    const gap = 8;
    let R;
    if (pic && pic.w > 0) {
      R = {
        left: { x: pad, y: pad, w: pic.x - pad * 2, h: B - pad * 2 },
        right: { x: pic.x + pic.w + pad, y: pad, w: A - pic.x - pic.w - pad * 2, h: B - pad * 2 },
        centre: { x: pic.x, y: pic.y + pic.h + 2, w: pic.w, h: B - pic.y - pic.h - 4 },
      };
    } else {
      // Three columns: each side at least as wide as its first group at the floor, the rest of
      // the width shared by how wide each side is at full size.
      const inner = A - pad * 2 - gap * 2, h = B - pad * 2;
      const need = (s) => (gs[s].length ? Math.min(...gs[s].map((G) => footprint(G, "l", closest(G, "l")).w)) : 0);
      const nat = (s) => (gs[s].length ? bbox(sides[s], "l").w : 0);
      const min = { left: need("left"), centre: need("centre"), right: need("right") };
      if (min.left + min.centre + min.right > inner) min.centre = 0;
      // Still too narrow: the two sides share it, and each keeps what fits.
      if (min.left + min.right > inner) min.left = min.right = inner / 2;
      const spare = Math.max(0, inner - min.left - min.centre - min.right);
      const want = ["left", "centre", "right"].map((s) => Math.max(0, nat(s) - min[s]));
      const sum = want.reduce((x, y) => x + y, 0) || 1;
      const w = {};
      ["left", "centre", "right"].forEach((s, i) => { w[s] = min[s] + (spare * want[i]) / sum; });
      R = {
        left: { x: pad, y: pad, w: w.left, h },
        centre: { x: pad + w.left + gap, y: pad, w: w.centre, h },
        right: { x: A - pad - w.right, y: pad, w: w.right, h },
      };
    }
    for (const s in R) if (R[s].w > 0 && R[s].h > 0) flow(out, gs[s], "l", R[s], gap);
    return out;
  };
})();
