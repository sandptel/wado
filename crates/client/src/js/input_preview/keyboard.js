// wado bridge — the Keyboard settings page's preview (spec §5.3, G15): a text line types a
// sentence, then acts out a shortcut — Super latches (the Morphing Toggle Rule: r1 → r2), "1"
// goes, and a good-tone chip says what the computer got. The key row is this phone's own
// (`W.kbPrefs`); a key just added pops in (`--pop`).

(() => {
  const TEXT = "Hello from the couch";
  function mount(el) {
    el.innerHTML = `<div class="vikline"><span class="vikt"></span><span class="vicaret"></span></div><div class="viksent"><span></span></div>
      <div class="vikmods"><span>Super</span><span>Ctrl</span><span>Alt</span><span>Shift</span></div><div class="vikkeys"></div><div class="vikphone"></div>`;
    const line = el.querySelector(".vikt"), sent = el.querySelector(".viksent span"), keysRow = el.querySelector(".vikkeys");
    const sup = el.querySelector(".vikmods span");
    let timer = 0, was = [];

    function keys(popped) {
      const now = W.kbPrefs.get().keys;
      keysRow.replaceChildren(...now.map((k) => {
        const s = document.createElement("span");
        s.textContent = W.kbPrefs.label(k);
        if (popped && !was.includes(k)) s.className = "enter";
        return s;
      }));
      was = now;
    }
    function play() {
      clearTimeout(timer);
      if (!el.isConnected) return;
      if (W.inputPreview.level() !== "full" || !W.inputPreview.running(el)) {
        line.textContent = TEXT;
        sent.textContent = "Super + 1 → workspace 1";
        sent.style.opacity = 1;
        timer = setTimeout(play, 2400);
        return;
      }
      let i = 0;
      sent.style.opacity = 0;
      const step = () => {
        if (!el.isConnected) return;
        if (i <= TEXT.length) { line.textContent = TEXT.slice(0, i++); W.inputPreview.text("keyboard", "typing"); timer = setTimeout(step, 70); return; }
        W.inputPreview.text("keyboard", "Super, then 1");
        sup.className = "latched";
        sup.animate([{ scale: 0.9 }, { scale: 1 }], { duration: 300, easing: "cubic-bezier(.34,1.56,.64,1)" });
        timer = setTimeout(() => {
          sup.className = "";
          sent.textContent = "Super + 1 → workspace 1";
          sent.animate([{ opacity: 0, translate: "0 6px" }, { opacity: 1, translate: "0 0", offset: 0.2 }, { opacity: 1, offset: 0.8 }, { opacity: 0 }],
            { duration: 1600, easing: "cubic-bezier(.32,.72,0,1)", fill: "both" });
          timer = setTimeout(() => { line.textContent = ""; play(); }, 2000);
        }, 900);
      };
      step();
    }
    keys(false);
    W.kbPrefs.onChange(() => { if (el.isConnected) keys(true); });
    play();
    return {
      demo(name) { if (name === "keys") { W.inputPreview.text("keyboard", "keys above the keyboard"); } },
      stop() { clearTimeout(timer); },
    };
  }
  W.inputPreview.register("keyboard", mount);
})();
