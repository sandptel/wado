// wado bridge — how the screens outside a session move: a computer's card growing into its page
// (and back), and a settings page pulled back off whatever it was pushed over.
//
// A hand-rolled FLIP rather than the View Transitions API: VT snapshots the page and is not
// interactive for the transition's length, and the brief is that motion never blocks input. Here
// the live page is under the finger from its first frame; everything moves by `transform` /
// `opacity` (WAAPI), and a second tap cancels what is running and starts from where it is.
//
//   W.heroOpen(key)    the card for `key` → its Computer page (Rust has just rendered it)
//   W.heroClose(key)   the page → back into its card, then {type:"viewBack"}
//   W.subClose()       a pushed settings page slides away, then {type:"viewBack"}
//
// Rust lays the screens out and owns where the person is; this file only moves them. Motion
// levels follow `data-motion` and `prefers-reduced-motion`: full, reduced (short crossfades, no
// travel), off (nothing moves at all).

(() => {
  const SPRING = "cubic-bezier(.32,.72,0,1)";
  const ACCEL = "cubic-bezier(.3,0,1,1)";
  const HERO = 520, HERO_OUT = 420;

  const level = () => {
    const m = document.documentElement.dataset.motion;
    if (m === "off") return 0;
    if (m === "reduced" || matchMedia("(prefers-reduced-motion: reduce)").matches) return 1;
    return 2;
  };
  let running = [];
  const A = (el, kf, o) => {
    if (!el) return null;
    const a = el.animate(kf, o);
    running.push(a);
    return a;
  };
  const stop = () => { for (const a of running) { try { a.cancel(); } catch (_) {} } running = []; };
  const buzz = (ms) => { try { if (navigator.vibrate) navigator.vibrate(ms); } catch (_) {} };
  // Rust renders, then this runs: wait the few frames until the element is there.
  const waitFor = (sel, n = 30) => new Promise((ok) => {
    const t = () => { const e = document.querySelector(sel); if (e || n-- <= 0) ok(e); else requestAnimationFrame(t); };
    t();
  });
  // A rect `s` as a transform on an element whose rest rect is `d`. `uni`: scale evenly (text).
  // Rects are on the glass; a translate is in the page's own px, which the interface size zooms
  // (size.css), so it is divided back.
  const tf = (s, d, uni) => {
    const sx = s.width / d.width, sy = uni ? sx : s.height / d.height;
    const z = document.getElementById("computer")?.currentCSSZoom || 1;
    return `translate(${(s.left - d.left) / z}px, ${(s.top - d.top) / z}px) scale(${sx}, ${sy})`;
  };
  const onScreen = (r) => r && r.width > 0 && r.bottom > 0 && r.top < innerHeight;
  const cardFor = (key) => [...document.querySelectorAll("#landing .ccard[data-key]")].find((c) => c.dataset.key === key) || null;
  const home = () => document.getElementById("landing");
  // A freshly rendered page holds itself invisible (CSS `cwait`) until motion takes it over —
  // so a frame of the finished page never flashes before the card has grown into it.
  const unveil = (c) => { for (const a of c.getAnimations()) if (a.animationName === "cwait") a.cancel(); };

  // The page's sections rise in after the hero, one stagger step apart.
  const rise = (c, base) => c.querySelectorAll(".cgrid .sx:not(.hero2)").forEach((e) => {
    const n = +getComputedStyle(e).getPropertyValue("--n") || 0;
    A(e, [{ opacity: 0, translate: "0 18px" }, { opacity: 1, translate: "0 0" }],
      { duration: 520, delay: base + n * 40, easing: SPRING, fill: "backwards" });
  });

  W.heroOpen = async (key) => {
    const c = await waitFor("#computer");
    if (!c) return;
    stop();
    unveil(c);
    c.scrollTop = 0;
    const lv = level();
    if (lv === 0) return;
    const card = cardFor(key);
    const cr = card && card.getBoundingClientRect();
    // From a widget, from Add a computer, or a card scrolled away: no element to share — fade.
    if (lv === 1 || !onScreen(cr) || document.getElementById("sub")) {
      A(c, [{ opacity: 0 }, { opacity: 1 }], { duration: lv === 1 ? 160 : 360, easing: SPRING });
      if (lv === 2) {
        rise(c, 120);
        A(home(), [{ scale: 1, opacity: 1 }, { scale: 0.94, opacity: 0 }], { duration: 360, easing: SPRING });
      }
      return;
    }
    const hero = c.querySelector(".hero2");
    const bg = hero.querySelector(".herobg"), hd = hero.querySelector(".hrow .disc"), hn = hero.querySelector(".hname");
    const sd = card.querySelector(".disc").getBoundingClientRect(), sn = card.querySelector(".cname").getBoundingClientRect();
    const db = bg.getBoundingClientRect(), dd = hd.getBoundingClientRect(), dn = hn.getBoundingClientRect();
    const o = { duration: HERO, easing: SPRING };
    // The card *is* the hero now: it steps out for the flight.
    A(card, [{ opacity: 0 }, { opacity: 0 }], { duration: HERO });
    A(bg, [{ transform: tf(cr, db, false) }, { transform: "none" }], o);
    A(bg.querySelector(".wash"), [{ opacity: 0 }, { opacity: 1 }], { duration: 360, delay: 120, easing: "ease-out", fill: "backwards" });
    A(hd, [{ transform: tf(sd, dd, true) }, { transform: "none" }], o);
    A(hn, [{ transform: tf(sn, dn, true) }, { transform: "none" }], o);
    hero.querySelectorAll(".hsub, .hmeta, .explain, .hops, .connerr").forEach((e) =>
      A(e, [{ opacity: 0, translate: "0 10px" }, { opacity: 1, translate: "0 0" }], { duration: 420, delay: 200, easing: SPRING, fill: "backwards" }));
    A(c.querySelector(".cground"), [{ opacity: 0 }, { opacity: 1 }], { duration: 300, easing: "ease-out" });
    A(c.querySelector(".cbar"), [{ opacity: 0 }, { opacity: 1 }], { duration: 300, delay: 200, easing: "ease-out", fill: "backwards" });
    A(home(), [{ scale: 1, opacity: 1 }, { scale: 0.94, opacity: 0 }], { duration: 360, easing: SPRING });
    rise(c, 160);
    buzz(6);
  };

  W.heroClose = (key) => {
    const done = () => emit({ type: "viewBack" });
    const c = document.getElementById("computer");
    const lv = level();
    stop();
    if (!c || lv === 0) return done();
    const card = cardFor(key);
    const cr = card && card.getBoundingClientRect();
    // Scrolled: the hero is no longer near its card, so flying it there would cross the screen.
    if (lv === 1 || c.scrollTop > 40 || !onScreen(cr)) {
      const a = A(c, [{ opacity: 1 }, { opacity: 0 }], { duration: lv === 1 ? 120 : 260, easing: SPRING, fill: "forwards" });
      if (lv === 2) A(home(), [{ scale: 0.96, opacity: 0 }, { scale: 1, opacity: 1 }], { duration: 380, easing: SPRING });
      a.finished.then(done, done);
      return;
    }
    const hero = c.querySelector(".hero2");
    const bg = hero.querySelector(".herobg"), hd = hero.querySelector(".hrow .disc"), hn = hero.querySelector(".hname");
    const sd = card.querySelector(".disc").getBoundingClientRect(), sn = card.querySelector(".cname").getBoundingClientRect();
    const db = bg.getBoundingClientRect(), dd = hd.getBoundingClientRect(), dn = hn.getBoundingClientRect();
    // Exits are quicker than entrances: the same spring, at about 80%.
    const o = { duration: HERO_OUT, easing: SPRING, fill: "forwards" };
    A(card, [{ opacity: 0 }, { opacity: 0 }], { duration: HERO_OUT + 120 });
    A(bg, [{ transform: "none" }, { transform: tf(cr, db, false) }], o);
    A(bg.querySelector(".wash"), [{ opacity: 1 }, { opacity: 0 }], { duration: 200, fill: "forwards" });
    A(hd, [{ transform: "none" }, { transform: tf(sd, dd, true) }], o);
    A(hn, [{ transform: "none" }, { transform: tf(sn, dn, true) }], o);
    c.querySelectorAll(".hsub, .hmeta, .explain, .hops, .connerr, .cgrid .sx:not(.hero2), .cbar").forEach((e) =>
      A(e, [{ opacity: 1 }, { opacity: 0 }], { duration: 160, easing: ACCEL, fill: "forwards" }));
    A(c.querySelector(".cground"), [{ opacity: 1 }, { opacity: 0 }], { duration: 300, delay: 80, fill: "forwards" });
    A(home(), [{ scale: 0.94, opacity: 0 }, { scale: 1, opacity: 1 }], { duration: HERO_OUT, easing: SPRING });
    setTimeout(done, HERO_OUT);
  };

  W.subClose = () => {
    const done = () => emit({ type: "viewBack" });
    const s = document.getElementById("sub");
    const lv = level();
    if (!s || lv === 0) return done();
    // The page under it comes back on its own CSS transition once Rust drops `under`.
    const a = s.animate(lv === 1 ? [{ opacity: 1 }, { opacity: 0 }] : [{ translate: "0 0" }, { translate: "100% 0" }],
      { duration: lv === 1 ? 120 : 340, easing: SPRING, fill: "forwards" });
    a.finished.then(done, done);
  };
})();
