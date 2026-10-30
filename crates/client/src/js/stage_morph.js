// wado bridge — the arrangement: console ⇄ cockpit (spec §6). Snapshot, apply, FLIP.
//
// One attribute decides the arrangement: `data-arr` on the root ("console" | "cockpit", absent
// outside a session). The controls are the same nodes in both — the console's CSS places them at
// their slots (`stage_fit.js`), the cockpit's is today's — so a morph is: read where everything
// is, flip the attribute, read where it all went, and animate each control back from where it
// was. Buttons fly (translate, plus scale where a size changed); plates do not move, they fade
// (CSS, `surround.css`); the picture is one transform from inset to identity.
//
// Hand-rolled rather than View Transitions: a VT snapshot is not interactive, and the picture
// must take touches from the first frame. Input maps against the video's rect at event time, so
// a tap mid-morph lands under the finger. Interruptible: a second tap reads the *current* visual
// rects (getBoundingClientRect includes running animations), cancels, and flips from there.
//
// Full screen does both jobs in one gesture: it asks for browser fullscreen (with the orientation
// lock, as before) and morphs — and still morphs if the browser refuses (iOS Safari outside a
// PWA). Leaving browser fullscreen any other way (Escape, the back gesture) morphs back.
//
//   W.stageMorph.to("console" | "cockpit")   from a tap (`[data-arrange]`), or the page
//   W.stageMorph.session(on)                  a session began (opened as `console { open }` says) or ended
//   W.stageMorph.prepare() / settle()         animate the next relayout (a face change)
//   W.stageMorph.motion()                     "full" | "reduced" | "off"

(() => {
  const M = (W.stageMorph = { pending: null });
  const root = document.documentElement;
  const SPRING = "cubic-bezier(.32, .72, 0, 1)";
  // Everything that exists in both arrangements and moves between them, by wave: the rails and
  // the strip's neighbours first (nearest the picture's edge, as if pushed), then status, then
  // the dock (spec S2).
  const FLY = [
    [0, ".rail .barbtn, #wado-bar .wsnum, #wado-bar .wsapp, #wado-pad .padbtn, #wado-pad .padstick"],
    [1, ".linkbadge .linkpill, #wado-here"],
    [2, "#wado-bar .grp.mid .barbtn, #wado-bar .grp.right .barbtn"],
  ];
  // Pills that carry words: they move, but are never stretched (their label would distort).
  const NO_SCALE = ".full, .linkpill, #wado-here";
  const running = [];

  M.motion = () => {
    const m = root.dataset.motion;
    if (m === "off" || m === "reduced") return m;
    try { if (matchMedia("(prefers-reduced-motion: reduce)").matches) return "reduced"; } catch (_) {}
    return "full";
  };

  let live = null;
  function announce(text) {
    if (!live || !live.isConnected) {
      live = document.createElement("div");
      live.id = "stage-announce";
      live.className = "sr-only";
      live.setAttribute("aria-live", "polite");
      document.body.appendChild(live);
    }
    live.textContent = text;
  }

  const video = () => document.getElementById("wado-video");

  function snapshot() {
    const els = new Map();
    for (const [wave, sel] of FLY) {
      document.querySelectorAll(sel).forEach((el) => {
        const r = el.getBoundingClientRect();
        if (r.width || r.height) els.set(el, { r, wave, o: +getComputedStyle(el).opacity });
      });
    }
    const v = video();
    return { els, pic: v ? v.getBoundingClientRect() : null };
  }

  function cancel() {
    while (running.length) { try { running.pop().cancel(); } catch (_) {} }
  }

  function track(a) { running.push(a); a.finished.then(() => { const i = running.indexOf(a); if (i >= 0) running.splice(i, 1); }, () => {}); }

  // Fly every control from its snapshot to where it is now.
  function fly(before, dur, toward) {
    for (const [el, b] of before.els) {
      if (!el.isConnected) continue;
      const a = el.getBoundingClientRect();
      if (!a.width && !a.height) continue; // gone from this arrangement: nothing to fly to
      const dx = b.r.left + b.r.width / 2 - (a.left + a.width / 2);
      const dy = b.r.top + b.r.height / 2 - (a.top + a.height / 2);
      const stretch = !el.matches(NO_SCALE);
      const sx = stretch && a.width ? b.r.width / a.width : 1;
      const sy = stretch && a.height ? b.r.height / a.height : 1;
      if (Math.abs(dx) < 0.5 && Math.abs(dy) < 0.5 && Math.abs(sx - 1) < 0.01 && Math.abs(sy - 1) < 0.01) continue;
      track(el.animate(
        [{ translate: `${dx}px ${dy}px`, scale: `${sx} ${sy}` }, { translate: "0px 0px", scale: "1 1" }],
        { duration: dur, delay: b.wave * 24, easing: SPRING, fill: "backwards" },
      ));
    }
    // The picture: one transform, start to finish — no reflow, no new output, no keyframe.
    const v = video();
    if (!v || !before.pic) return;
    const host = v.offsetParent ? v.offsetParent.getBoundingClientRect() : { left: 0, top: 0 };
    const w0 = v.offsetWidth || 1;
    const b = before.pic;
    const k0 = b.width / w0;
    const from = `translate(${b.left - host.left}px, ${b.top - host.top}px) scale(${k0})`;
    const cs = getComputedStyle(v).transform;
    const to = cs && cs !== "none" ? cs : "translate(0px, 0px) scale(1)";
    let frames;
    if (toward === "cockpit") {
      // Anticipation: a 1.5% inhale about its own centre over the first 14%, then the growth.
      const k = 0.985;
      const ax = b.left - host.left + (b.width * (1 - k)) / 2, ay = b.top - host.top + (b.height * (1 - k)) / 2;
      frames = [{ transform: from, easing: SPRING }, { transform: `translate(${ax}px, ${ay}px) scale(${k0 * k})`, offset: 0.14, easing: SPRING }, { transform: to }];
      track(v.animate(frames, { duration: dur, easing: "linear" }));
    } else {
      track(v.animate([{ transform: from }, { transform: to }], { duration: dur, easing: SPRING }));
    }
  }

  // Reduced motion: no travel at all, the new arrangement fades in.
  function fadeIn(dur) {
    const els = [video(), document.getElementById("surround"), document.getElementById("wado-bar"),
      ...document.querySelectorAll(".rail, .linkbadge, #wado-here, #wado-pad")].filter(Boolean);
    for (const el of els) track(el.animate([{ opacity: 0 }, { opacity: 1 }], { duration: dur, easing: "linear" }));
  }

  function play(before, toward, isMorph) {
    const mo = M.motion();
    if (mo === "off") return;
    if (mo === "reduced") { fadeIn(160); return; }
    // The tokens (`base.css`): --dur-stage to the cockpit, --dur-stage-out back.
    // A time token may read back as "620ms" or "0.62s".
    const tok = (k, d) => {
      const v = getComputedStyle(root).getPropertyValue(k).trim();
      const n = parseFloat(v);
      return n > 0 ? (/ms$/.test(v) ? n : /s$/.test(v) ? n * 1000 : n) : d;
    };
    const dur = isMorph ? (toward === "cockpit" ? tok("--dur-stage", 620) : tok("--dur-stage-out", 520)) : 420;
    root.style.setProperty("--dur-stage-now", dur + "ms");
    fly(before, dur, toward);
    // Ghost pointers follow the picture once it has landed.
    setTimeout(() => { if (W.presenceRefit) W.presenceRefit(); if (W.deckPad) W.deckPad.refit(); }, dur + 40);
  }

  function set(arr) {
    root.dataset.arr = arr;
    if (W.stageFit) W.stageFit.inert();
    emit({ type: "arrangement", arr });
    if (W.presenceRefit) W.presenceRefit();
    if (W.deckPad) W.deckPad.refit();
  }

  function waitResize(ms) {
    return new Promise((resolve) => {
      const done = () => { removeEventListener("resize", done); clearTimeout(t); resolve(); };
      const t = setTimeout(done, ms);
      addEventListener("resize", done);
    });
  }

  let seq = 0;
  let leaving = false; // our own exitFullscreen: its fullscreenchange is not an Escape
  M.to = async (arr, opts = {}) => {
    if (!root.dataset.arr) return; // no session: nothing to arrange
    const from = root.dataset.arr || "console";
    if (arr === from && !opts.force) return;
    const my = ++seq;
    if (arr === "cockpit") {
      const before = snapshot();
      cancel();
      // Into browser fullscreen first, inside the tap; the morph does not wait for a refusal.
      if (!document.fullscreenElement && opts.fullscreen !== false) {
        const el = document.querySelector(".app") || root;
        let entered = false;
        try { await el.requestFullscreen({ navigationUI: "hide" }); entered = true; } catch (_) {}
        if (entered) {
          const v = video();
          if (v && v.videoWidth && v.videoHeight) {
            try { await screen.orientation.lock(v.videoWidth >= v.videoHeight ? "landscape" : "portrait"); } catch (_) {}
          }
          await waitResize(250);
        }
      }
      if (my !== seq) return;
      set("cockpit");
      announce("Full screen. The controls float over the picture.");
      play(before, "cockpit", from !== "cockpit");
    } else {
      // Out of browser fullscreen first, then lay the console out for the viewport that is left.
      let resized = !!opts.resized;
      if (document.fullscreenElement && opts.fullscreen !== false) {
        leaving = true;
        try { await document.exitFullscreen(); } catch (_) {}
        resized = true;
      }
      if (resized) {
        await waitResize(250);
        if (W.stageFit) { W.stageFit.report(); await W.stageFit.next(300); }
      }
      leaving = false;
      if (my !== seq) return;
      const before = snapshot();
      cancel();
      set("console");
      announce("Console. The controls sit around the picture.");
      play(before, "console", from !== "console");
    }
  };

  M.toggle = () => M.to(root.dataset.arr === "cockpit" ? "console" : "cockpit");

  // A relayout within the console (a face change): snapshot now, fly when the new slots land.
  M.prepare = () => {
    if (root.dataset.arr !== "console") return;
    M.pending = snapshot();
    setTimeout(() => { M.pending = null; }, 1500);
  };
  M.settle = () => {
    const before = M.pending;
    M.pending = null;
    if (!before) return;
    cancel();
    play(before, "console", false);
  };

  M.session = (on) => {
    if (!on) {
      cancel();
      if (W.deckKeys) W.deckKeys.releaseAll();
      if (W.deckPad) W.deckPad.release();
      delete root.dataset.arr;
      if (W.stageFit) W.stageFit.inert();
      return;
    }
    if (root.dataset.arr) return; // already arranged: a reconnect keeps what the person chose
    const open = W.consolePrefs ? W.consolePrefs.get().open : "console";
    root.dataset.arr = open === "full" ? "cockpit" : "console";
    if (W.stageFit) { W.stageFit.inert(); W.stageFit.report(); }
    emit({ type: "arrangement", arr: root.dataset.arr });
  };

  // The toggle, and the cockpit rail's face button, inside the tap (the fullscreen request needs
  // the user gesture; the Rust handler would run after it).
  document.addEventListener("click", (e) => {
    const b = e.target && e.target.closest && e.target.closest("[data-arrange]");
    if (!b) return;
    e.preventDefault();
    const want = b.dataset.arrange;
    if (want === "toggle") M.toggle();
    else M.to(want);
  }, true);

  // Escape or the back gesture left browser fullscreen: back to the console.
  document.addEventListener("fullscreenchange", () => {
    if (!document.fullscreenElement && !leaving && root.dataset.arr === "cockpit") {
      M.to("console", { fullscreen: false, resized: true });
    }
  });
})();
