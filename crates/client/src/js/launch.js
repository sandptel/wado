// wado bridge — "the poster becomes the game" (ui/home/launch.rs): the launch layer grows out of
// the poster tapped, FLIP style — measured from the poster and from where the layer itself ends up
// (the whole screen on a phone, a centred card on a big screen), then let go on the spring.
//
//   W.launchGrow(key)   key: the poster's data-poster; "" grows from the centre

W.launchGrow = (key) => {
  const from = key ? document.querySelector(`[data-poster="${CSS.escape(key)}"]`) : null;
  const r = from ? from.getBoundingClientRect() : null;
  let tries = 0;
  const go = () => {
    const el = document.getElementById("glaunch");
    if (!el) { if (tries++ < 30) requestAnimationFrame(go); return; }
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (!r || still) { el.classList.add("in"); return; }
    const t = el.getBoundingClientRect();
    el.style.transition = "none";
    el.style.transformOrigin = "0 0";
    el.style.transform = `translate(${r.left - t.left}px, ${r.top - t.top}px) scale(${r.width / t.width}, ${r.height / t.height})`;
    el.getBoundingClientRect();
    el.style.transition = "";
    el.style.transform = "";
    el.classList.add("in");
  };
  requestAnimationFrame(go);
};
