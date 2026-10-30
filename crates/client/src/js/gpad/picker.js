// wado bridge — the layout picker (spec §5.7): a sheet (portrait bottom, landscape right side, so
// the pad stays visible). Tap one to try it — it morphs behind the sheet (G6). Away from a session
// "Same as in a session" comes first; then the built-ins, then yours, with "Copy this one".
//
//   W.gpadPicker.open() · close(now) · isOpen()

(() => {
  const V = W.gpadVariants;
  const K = (W.gpadPicker = {});
  const G = W.gpadScreen;
  let shown = false;

  K.isOpen = () => shown;

  function card(item, L, checked) {
    const b = W.gpadVariantCard(item, L, checked);
    b.addEventListener("click", () => pick(item.id, b));
    if (item.mine) {
      const x = document.createElement("span");
      x.className = "iconbtn gpvdel";
      x.setAttribute("role", "button");
      x.setAttribute("tabindex", "0");
      x.setAttribute("aria-label", `Delete ${item.name}`);
      x.innerHTML = W.padIcon("trash");
      const del = (e) => {
        e.stopPropagation();
        W.gpadPrefs.remove(item.id);
        G.say(`${item.name} deleted`);
        render();
      };
      x.addEventListener("click", del);
      x.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") del(e); });
      b.appendChild(x);
    }
    return b;
  }

  function current() {
    const p = W.gpadPrefs.get();
    return G.S.where === "away" ? p.away : p.layout;
  }

  function pick(id, b) {
    const sheet = G.el.sheet;
    sheet.querySelectorAll(".gpvcard").forEach((x) => x.setAttribute("aria-checked", String(x === b)));
    if (id === current()) return;
    G.flush();
    W.gpadPrefs.select(G.S.where, id);
    G.S.undo = [];
    G.S.redo = [];
    const next = W.gpadPrefs.layoutFor(G.S.where);
    G.morphTo(next);
    G.say(`Layout: ${next.name}`);
    if (navigator.vibrate && W.gpadPrefs.get().haptics !== "off") { try { navigator.vibrate(6); } catch (_) {} }
  }

  function render() {
    const sheet = G.el.sheet, o = G.o();
    const cur = current();
    const p = W.gpadPrefs.get();
    sheet.innerHTML = `<div class="gpgrab" aria-hidden="true"></div><h4>Layouts</h4><p class="gpssub">Tap one to try it. It changes behind this sheet.</p>
      <div class="sect"><span>Built in</span></div><div class="gpvgrid gpbuilt" role="radiogroup" aria-label="Built-in layouts"></div>
      <div class="sect"><span>Yours</span></div><div class="gpvgrid gpmine" role="radiogroup" aria-label="Your layouts"></div>`;
    const built = sheet.querySelector(".gpbuilt"), mine = sheet.querySelector(".gpmine");
    const cards = [];
    if (G.S.where === "away") {
      const L = W.gpadPrefs.layoutOf(p.layout);
      cards.push([built, card({ id: V.SAME, name: "Same as in a session", sub: L.name, badge: "session" }, L, cur === V.SAME)]);
    }
    for (const it of W.gpadPrefs.list()) cards.push([it.mine ? mine : built, card(it, W.gpadPrefs.layoutOf(it.id), cur === it.id)]);
    for (const [box, b] of cards) box.appendChild(b);
    const add = document.createElement("button");
    add.className = "gpvcard gpvadd";
    add.innerHTML = `${W.padIcon("plus")}<span>Copy this one</span>`;
    add.addEventListener("click", () => {
      G.flush();
      const id = W.gpadPrefs.saveAs(G.S.L, null);
      if (!id) { G.say("Eight layouts is the most — delete one first."); return; }
      W.gpadPrefs.select(G.S.where, id);
      G.S.L = W.gpadPrefs.layoutFor(G.S.where);
      K.close(true);
      W.gpadEdit.set(true);
      G.say(`Copied as "${G.S.L.name}". Arrange it, then Done.`);
    });
    mine.appendChild(add);
    requestAnimationFrame(() => {
      for (const [, b] of cards) W.gpadThumb(b.querySelector(".gpvthumb"), b._L, o === "p" ? "p" : "l");
    });
  }

  K.open = () => {
    if (!G.S) return;
    render();
    shown = true;
    G.el.sheet.classList.add("on");
    G.el.scrim.classList.add("on");
    G.el.scrim.onclick = () => K.close(true);
    const first = G.el.sheet.querySelector('[aria-checked="true"]') || G.el.sheet.querySelector(".gpvcard");
    if (first) setTimeout(() => first.focus({ preventScroll: true }), 50);
  };
  K.close = () => {
    if (!shown) return;
    shown = false;
    G.el.sheet.classList.remove("on");
    G.el.scrim.classList.remove("on");
  };
})();
