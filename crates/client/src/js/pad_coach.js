// wado bridge — the first-use coach: the first time this browser opens the trackpad, a short
// looping picture over the pad shows the three things it does — one finger moves the pointer,
// two fingers scroll, three fingers swipe — and the first touch dismisses it for good (as that
// touch, which still reaches the computer: the coach never takes a pointer event).
//
//   W.padCoach.attach(area) / detach()   attach shows it only if it has not been dismissed
//   W.padCoach.reset()                   show it again next time

(() => {
  const C = (W.padCoach = {});
  const SEEN = "wado.padCoach";
  const SCENES = [
    [1, "One finger moves the pointer"],
    [2, "Two fingers scroll"],
    [3, "Three fingers swipe, as on the laptop"],
  ];
  let area = null;
  let n = null;

  const seen = () => { try { return localStorage.getItem(SEEN) === "1"; } catch (_) { return true; } };
  const remember = () => { try { localStorage.setItem(SEEN, "1"); } catch (_) {} };
  C.reset = () => { try { localStorage.removeItem(SEEN); } catch (_) {} };

  function dismiss() {
    remember();
    if (!n) return;
    const gone = n;
    n = null;
    gone.classList.add("out");
    setTimeout(() => gone.remove(), 260);
    if (area) area.removeEventListener("pointerdown", dismiss);
  }

  C.attach = (node) => {
    C.detach();
    area = node;
    if (seen()) return;
    n = document.createElement("div");
    n.className = "tpcoach";
    n.setAttribute("role", "note");
    n.innerHTML = SCENES.map(([fingers, words], i) =>
      `<div class="tpcoach-scene" style="--s:${i}">` +
        `<div class="tpcoach-hand" data-n="${fingers}">${"<i></i>".repeat(fingers)}</div>` +
        `<p>${words}</p>` +
      `</div>`).join("") +
      `<p class="tpcoach-foot">Touch anywhere to start</p>`;
    area.appendChild(n);
    area.addEventListener("pointerdown", dismiss);
  };

  C.detach = () => {
    if (area) area.removeEventListener("pointerdown", dismiss);
    if (n) { n.remove(); n = null; }
    area = null;
  };
})();
