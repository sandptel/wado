// wado bridge — the Touchpad settings page's preview (spec §5.2, G15): the phone drawn the way you
// hold it, with its rail, beside a mini screen. Demos act out what the settings do: slide to move
// (pointer travel = finger travel × speed), two fingers scroll, tap to click, an edge swipe or a
// hold of two running its action (a toast names it). Each change on the page re-runs its demo;
// otherwise the slide plays every 2.4 s while the page is up and motion is full.
//
// Built from the trackpad's own settings (`W.padPrefs`) on every run, so it shows what is set.

(() => {
  const SPRING = "cubic-bezier(.32,.72,0,1)";
  const CAPS = {
    glide: "slide to move", scroll: "two fingers scroll", tap: "tap to click",
    edge: "edge swipe → action", hold: "hold two → action", keys: "keys beside the pad",
  };
  const RAIL_KEYS = ["esc", "tab", "super", "ctrl", "alt"];

  function mount(el) {
    el.innerHTML = `<div class="vitphone"><div class="vitrail"></div><div class="vitsurf"><span class="vifing"></span><span class="vifing"></span></div></div>
      <div class="vitscr"><div class="vitws"><div class="vitwin"><i></i><i style="width:60%"></i><i style="width:75%"></i><i style="width:40%"></i></div></div>
      <span class="viptr"></span><span class="viring"></span><span class="vittoast"></span></div>`;
    const q = (s) => el.querySelector(s);
    const phone = q(".vitphone"), rail = q(".vitrail"), surf = q(".vitsurf"), scr = q(".vitscr"), ws = q(".vitws"),
      ptr = q(".viptr"), ring = q(".viring"), toast = q(".vittoast");
    const [f1, f2] = el.querySelectorAll(".vifing");
    let timer = 0, current = "glide";
    const home = [0.3, 0.55];

    function paint() {
      const p = W.padPrefs.get();
      phone.classList.toggle("upright", !p.landscape);
      rail.dataset.rail = p.rail;
      const names = p.rail === "actions" ? (W.padActions && W.padActions.list.length
        ? W.padRail.items(p.quick, W.padActions.list).map((a) => a.name.split(" ")[0])
        : ["‹ ws", "ws ›", "full", "⏯"]) : RAIL_KEYS;
      rail.replaceChildren(...names.slice(0, 5).map((t) => { const s = document.createElement("span"); s.textContent = t; return s; }));
    }

    function run(name) {
      current = name || current;
      paint();
      W.inputPreview.text("touchpad", CAPS[current] || current);
      const W_ = surf.clientWidth, H = surf.clientHeight, SW = scr.clientWidth, SH = scr.clientHeight;
      if (!W_ || !SW) return;
      for (const e of [f1, f2, ptr, ring, ws, toast]) for (const a of e.getAnimations()) a.cancel();
      const P = (x, y) => `translate(${x}px, ${y}px)`;
      const px = home[0] * SW, py = home[1] * SH;
      ptr.style.transform = P(px, py);
      if (W.inputPreview.level() !== "full") { f1.style.opacity = 1; f1.style.transform = P(W_ * 0.45, H * 0.5); return; }
      f1.style.opacity = "";
      const D = 1400, A = (e, kf, o) => e.animate(kf, { duration: D, fill: "both", ...o });
      const speed = W.padPrefs.get().speed || 1;
      if (current === "glide" || current === "keys") {
        const x0 = W_ * 0.25, y0 = H * 0.66, x1 = W_ * 0.72, y1 = H * 0.36;
        A(f1, [{ transform: P(x0, y0), opacity: 0 }, { transform: P(x0, y0), opacity: 1, offset: 0.12 }, { transform: P(x1, y1), opacity: 1, offset: 0.75 }, { transform: P(x1, y1), opacity: 0 }], { easing: "linear" });
        // The pointer travels finger distance × speed (scaled to the mini screen).
        const k = 0.62 * speed * (SW / W_);
        const tx = Math.min(SW - 8, Math.max(4, px + (x1 - x0) * k)), ty = Math.min(SH - 12, Math.max(4, py + (y1 - y0) * k));
        A(ptr, [{ transform: P(px, py) }, { transform: P(px, py), offset: 0.12 }, { transform: P(tx, ty), offset: 0.75 }, { transform: P(tx, ty) }], { easing: "linear" });
      } else if (current === "scroll") {
        [f1, f2].forEach((f, i) => A(f, [{ transform: P(W_ * (0.42 + i * 0.16), H * 0.72), opacity: 0 }, { opacity: 1, offset: 0.12, transform: P(W_ * (0.42 + i * 0.16), H * 0.72) },
          { transform: P(W_ * (0.42 + i * 0.16), H * 0.3), opacity: 1, offset: 0.75 }, { opacity: 0, transform: P(W_ * (0.42 + i * 0.16), H * 0.3) }]));
        A(ws.firstElementChild, [{ translate: "0 0" }, { translate: "0 0", offset: 0.12 }, { translate: "0 -22px", offset: 0.75 }, { translate: "0 -22px" }], { easing: SPRING });
      } else if (current === "tap") {
        A(f1, [{ transform: P(W_ / 2, H / 2) + " scale(.6)", opacity: 0 }, { transform: P(W_ / 2, H / 2), opacity: 1, offset: 0.2 }, { transform: P(W_ / 2, H / 2), opacity: 0, offset: 0.45 }, { opacity: 0 }], { easing: SPRING });
        A(ring, [{ transform: P(px + 2, py + 2) + " scale(.4)", opacity: 0 }, { transform: P(px + 2, py + 2) + " scale(.4)", opacity: 0.9, offset: 0.25 },
          { transform: P(px + 2, py + 2) + " scale(1.6)", opacity: 0, offset: 0.7 }, { opacity: 0 }], { easing: SPRING });
      } else if (current === "edge" || current === "hold") {
        const g = W.padPrefs.get().gestures;
        const id = current === "edge" ? g["edge-left"] || g["edge-right"] : g["two-hold"];
        const a = id && W.padActions && W.padActions.list.find((x) => x.id === id);
        toast.textContent = a ? a.name : "Off — pick an action below";
        if (current === "edge") {
          A(f1, [{ transform: P(0, H / 2), opacity: 0 }, { opacity: 1, offset: 0.1, transform: P(0, H / 2) }, { transform: P(W_ * 0.45, H / 2), opacity: 1, offset: 0.55 }, { opacity: 0, transform: P(W_ * 0.45, H / 2) }], { easing: SPRING });
          if (a) A(ws, [{ translate: "0 0", opacity: 1 }, { translate: "0 0", offset: 0.5 }, { translate: "-30% 0", opacity: 0, offset: 0.62 }, { translate: "30% 0", opacity: 0, offset: 0.63 }, { translate: "0 0", opacity: 1 }], { easing: SPRING });
        } else {
          [f1, f2].forEach((f, i) => A(f, [{ transform: P(W_ * (0.4 + i * 0.18), H / 2), opacity: 0 }, { opacity: 1, offset: 0.1 },
            { opacity: 1, transform: P(W_ * (0.4 + i * 0.18), H / 2) + " scale(1.15)", offset: 0.55 }, { opacity: 0, transform: P(W_ * (0.4 + i * 0.18), H / 2) }], { easing: SPRING }));
        }
        A(toast, [{ opacity: 0 }, { opacity: 0, offset: 0.55 }, { opacity: 1, offset: 0.65 }, { opacity: 1, offset: 0.92 }, { opacity: 0 }], { duration: D + 300 });
      }
    }

    const loop = () => {
      clearInterval(timer);
      timer = setInterval(() => {
        if (!el.isConnected) { clearInterval(timer); return; }
        if (W.inputPreview.running(el) && W.inputPreview.level() === "full") run();
      }, 2400);
    };
    requestAnimationFrame(() => { run("glide"); loop(); });
    const re = () => { if (el.isConnected) paint(); };
    W.padPrefs.onChange(re);
    return {
      demo(name) { run(name); loop(); },
      stop() { clearInterval(timer); },
    };
  }

  W.inputPreview.register("touchpad", mount);

  // The gesture pickers on the same page are the trackpad's own (`pad_settings.js`); picking one
  // plays its gesture above.
  W.inputPreview.register("tp-gestures", (el) => {
    const draw = () => { if (el.isConnected) W.padSettings.renderInto(el, "gestures"); };
    draw();
    if (W.padActions) W.padActions.onChange(draw);
    if (!W.padActions.list.length && !W.padActions.denied && W.actionsGet) W.actionsGet();
    el.addEventListener("change", (e) => {
      const g = e.target && e.target.id ? e.target.id.replace("tpsg-", "") : "";
      W.inputPreview.demo("touchpad", g === "two-hold" ? "hold" : g.startsWith("edge") ? "edge" : "tap");
    });
    el.addEventListener("focusin", (e) => {
      const g = e.target && e.target.id ? e.target.id.replace("tpsg-", "") : "";
      if (g) W.inputPreview.demo("touchpad", g === "two-hold" ? "hold" : g.startsWith("edge") ? "edge" : "tap");
    });
    return {};
  });
})();
