// wado bridge — where a layout's controls land on a screen (spec §9.3). Pure: boxes are numbers.
//
// Positions are designed on a reference phone (`REF`) and re-anchored on any other box: a group in
// the left third keeps its px distance from the left edge, the right third from the right, the
// middle third from the centre (the same for rows). Members keep their px offsets from the group's
// centre. So on a tablet the sticks stay under the thumbs and a cluster never spreads apart; only
// the empty middle grows (D8). `k` scales everything: 1 on the play screen, box/REF for a thumbnail.
//
// The Pocket Console's deck is a box too small for a phone layout (§4): `deck()` packs each side's
// groups into its own part of the box — left, centre, right — keeping their relative offsets, size,
// shape and look, scaled down only as far as the 44 px floor allows.
//
//   W.gpadPlace.orient(w, h)                       "l" | "p"
//   W.gpadPlace.at(c, L, o, k, w, h)               → [cx, cy] in box px
//   W.gpadPlace.size(c, o, k)                      → [w, h] in px
//   W.gpadPlace.all(L, o, k, w, h)                 → Map id → {x, y, w, h} (top-left box)
//   W.gpadPlace.deck(L, w, h)                      → the same, packed for a console deck
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

  // The smallest k at which every control in `cs` is still 44 px on its short side.
  const floorK = (cs, o) => Math.max(0, ...cs.map((c) => {
    const [w, h] = P.size(c, o, 1);
    return V.MIN_PX / Math.min(w, h);
  }));

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

  // Lay `cs` out at scale k with their bbox's top-left at (x0, y0).
  function lay(out, cs, o, k, bb, x0, y0) {
    const [DW, DH] = V.REF[o];
    for (const c of cs) {
      const [w, h] = P.size(c, o, k);
      const cx = x0 + (c[o][0] * DW - bb.l) * k, cy = y0 + (c[o][1] * DH - bb.t) * k;
      out.set(c.id, { x: cx - w / 2, y: cy - h / 2, w, h });
    }
  }

  P.side = (L, c, o) => {
    const gx = P.groupCentre(L, c.g, o)[0];
    return gx < 1 / 3 ? "left" : gx > 2 / 3 ? "right" : "centre";
  };

  P.deck = (L, A, B, pad = 6) => {
    const out = new Map();
    const shown = L.controls.filter((c) => !c.hidden);
    if (!shown.length) return out;
    // A tall deck (a column beside the picture) takes the portrait arrangement whole.
    if (B > A) {
      const bb = bbox(shown, "p");
      const k = Math.max(floorK(shown, "p"), Math.min(1, (A - pad * 2) / bb.w, (B - pad * 2) / bb.h));
      lay(out, shown, "p", k, bb, (A - bb.w * k) / 2, (B - bb.h * k) / 2);
      return out;
    }
    const sides = { left: [], centre: [], right: [] };
    for (const c of shown) sides[P.side(L, c, "l")].push(c);
    const boxes = {};
    for (const s in sides) if (sides[s].length) boxes[s] = bbox(sides[s], "l");
    const gap = 10;
    const used = Object.values(boxes);
    const sumW = used.reduce((a, b) => a + b.w, 0) + gap * Math.max(0, used.length - 1);
    const maxH = Math.max(...used.map((b) => b.h));
    const k = Math.max(floorK(shown, "l"), Math.min(1, (A - pad * 2) / sumW, (B - pad * 2) / maxH));
    for (const s in boxes) {
      const bb = boxes[s];
      const x0 = s === "left" ? pad : s === "right" ? A - pad - bb.w * k : (A - bb.w * k) / 2;
      lay(out, sides[s], "l", k, bb, x0, (B - bb.h * k) / 2);
    }
    return out;
  };
})();
