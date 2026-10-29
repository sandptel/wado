// wado bridge — the trackpad's "that ran" moment: when a quick action or a phone gesture runs an
// action, its icon and name pop up over the pad with a tick, or shake with a cross, and are gone
// in about 600 ms. One at a time; a new one replaces the last. The words of a failure go to the
// alert card as well (js/pad_ui.js), since this is gone before anyone could read a sentence.
//
//   W.padConfirm.show(area, {icon, name}, {ok}, at?)   at: {x, y} in the pad's own px, else centre

(() => {
  const C = (W.padConfirm = {});
  const LIFE_MS = 620;
  let cur = null;

  C.show = (area, action, res, at) => {
    if (!area) return;
    if (cur) { cur.remove(); cur = null; }
    const n = document.createElement("div");
    n.className = "tpconfirm " + (res.ok ? "ok" : "bad");
    n.setAttribute("role", "status");
    const disc = document.createElement("div");
    disc.className = "tpconfirm-disc";
    disc.innerHTML = W.padIcon(action.icon) +
      `<span class="tpconfirm-badge">${W.padIcon(res.ok ? "check" : "x")}</span>`;
    const name = document.createElement("div");
    name.className = "tpconfirm-name";
    name.textContent = (action.name || "") + (res.ok ? "" : " — didn't work");
    n.append(disc, name);
    if (at) n.style.transform = `translate(${at.x}px, ${at.y}px)`;
    else n.classList.add("mid");
    area.appendChild(n);
    cur = n;
    setTimeout(() => { n.remove(); if (cur === n) cur = null; }, LIFE_MS);
  };
})();
