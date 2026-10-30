// wado bridge — "for how long": any `[data-elapsed]` element shows the time since it appeared
// (`0:42`, `3:05`), ticked here once a second so the Rust UI never re-renders for a clock. The
// session's in-stage state (`ui/surround/stage_state.rs`) is the first user.

(() => {
  const seen = new WeakMap();
  const fmt = (s) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  setInterval(() => {
    const els = document.querySelectorAll("[data-elapsed]");
    if (!els.length) return;
    const now = Date.now();
    for (const el of els) {
      if (!seen.has(el)) seen.set(el, now);
      const s = Math.floor((now - seen.get(el)) / 1000);
      const t = s < 5 ? "" : "Waiting " + fmt(s);
      if (el.textContent !== t) el.textContent = t;
    }
  }, 1000);
})();
