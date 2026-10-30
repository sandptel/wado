// wado bridge — the hub's three live miniatures (spec §5.1): the touchpad (a finger slides, the
// pointer on a mini screen follows), the keyboard (a line types itself above the modifier and
// key rows), the gamepad (the layout in use, a face button pressing every 2 s). Ambient loops:
// paused off-screen and under reduced motion, where they hold one still frame.

(() => {
  const IP = W.inputPreview;
  const SPRING = "cubic-bezier(.32,.72,0,1)";
  const ambient = (el, every, tick) => {
    tick();
    const t = setInterval(() => {
      if (!el.isConnected) { clearInterval(t); return; }
      if (IP.running(el) && IP.level() === "full") tick();
    }, every);
    return { stop() { clearInterval(t); } };
  };

  IP.register("mini-touchpad", (el) => {
    el.innerHTML = '<div class="vimpad"></div><div class="vimscr"></div><span class="vimptr"></span><span class="vimfing"></span>';
    const f = el.querySelector(".vimfing"), p = el.querySelector(".vimptr");
    return ambient(el, 2000, () => {
      if (IP.level() !== "full") return;
      const w = el.clientWidth;
      f.animate([{ translate: "0 0", opacity: 0.4 }, { translate: "0 0", opacity: 1, offset: 0.15 }, { translate: `${w * 0.2}px -34px`, offset: 0.7 }, { translate: "0 0", opacity: 1 }], { duration: 1600, easing: SPRING });
      p.animate([{ translate: "0 0" }, { translate: "0 0", offset: 0.15 }, { translate: `${w * 0.12}px -22px`, offset: 0.7 }, { translate: "0 0" }], { duration: 1600, easing: SPRING });
    });
  });

  IP.register("mini-keyboard", (el) => {
    el.innerHTML = '<div class="vimtext"><span></span><span class="vicaret"></span></div><div class="vimrow vimmods"><span>super</span><span>ctrl</span><span>alt</span><span>shift</span></div><div class="vimrow vimkeys"></div>';
    const t = el.querySelector(".vimtext span"), row = el.querySelector(".vimkeys");
    const keys = () => row.replaceChildren(...W.kbPrefs.get().keys.slice(0, 6).map((k) => { const s = document.createElement("span"); s.textContent = W.kbPrefs.label(k).toLowerCase(); return s; }));
    keys();
    W.kbPrefs.onChange(() => el.isConnected && keys());
    const words = ["git push", "Hello!", "ls -la"];
    let w = 0, c = 0;
    if (IP.level() !== "full") { t.textContent = words[0]; return {}; }
    return ambient(el, 140, () => {
      const s = words[w % words.length];
      t.textContent = s.slice(0, c++);
      if (c > s.length + 8) { c = 0; w++; }
    });
  });

  IP.register("mini-gamepad", (el) => {
    const draw = () => { if (el.isConnected) W.gpadThumb(el, W.gpadPrefs.layoutFor("away"), "l"); };
    requestAnimationFrame(draw);
    W.gpadPrefs.onChange(draw);
    return ambient(el, 2000, () => {
      if (!el._gp) return;
      const faces = [...el._gp.values()].filter((e) => e._c.kind === "btn");
      const e = faces[(Math.random() * faces.length) | 0];
      if (!e) return;
      e.classList.add("on");
      setTimeout(() => e.classList.remove("on"), 220);
    });
  });
})();
