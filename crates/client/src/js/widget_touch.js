// wado bridge — the Trackpad widget's surface answers a finger: a ring blooms from where it
// touched (scale .4 → 2.2, fading, decelerating) before the trackpad opens. Off with motion off.

document.addEventListener("pointerdown", (e) => {
  const pad = e.target.closest && e.target.closest(".padw");
  if (!pad || document.documentElement.dataset.motion === "off" || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const r = pad.getBoundingClientRect();
  const t = document.createElement("i");
  t.className = "touch";
  t.style.left = e.clientX - r.left + "px";
  t.style.top = e.clientY - r.top + "px";
  pad.appendChild(t);
  setTimeout(() => t.remove(), 600);
}, true);
