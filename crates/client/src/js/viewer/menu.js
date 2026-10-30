// wado bridge — the viewer's overflow menu (⋯): everything that acts on the file, in one place.
//
//   V.menu(button, items)    open a menu under `button`; items: [{ label, icon, run }]
//   V.menuClose()
//
// A real menu: arrow keys move, Escape or a tap outside closes it and gives focus back to ⋯.

(() => {
  const F = W.files;
  const V = F.viewer;
  const h = F.h;
  let open = null;

  V.menuClose = (refocus = true) => {
    if (!open) return;
    const { box, catcher, button } = open;
    open = null;
    button.setAttribute("aria-expanded", "false");
    catcher.remove();
    box.classList.add("out");
    const gone = () => box.remove();
    box.addEventListener("animationend", gone, { once: true });
    setTimeout(gone, 260);
    if (refocus) button.focus();
  };
  V.menuOpen = () => !!open;

  V.menu = (button, items) => {
    if (open) return V.menuClose();
    const layer = button.closest(".fview") || document.body;
    const catcher = h("div", { class: "fvmenucatch", onclick: () => V.menuClose() });
    const rows = items.map((it) => h("button", { class: "fvmenuitem", role: "menuitem", tabindex: "-1",
      onclick: () => { V.menuClose(false); it.run(); } }, V.ico(it.icon), h("span", {}, it.label)));
    const box = h("div", { class: "fvmenu", role: "menu", "aria-label": "File actions" }, ...rows);
    box.addEventListener("keydown", (ev) => {
      const i = rows.indexOf(document.activeElement);
      if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
        ev.preventDefault();
        rows[(i + (ev.key === "ArrowDown" ? 1 : rows.length - 1)) % rows.length].focus();
      } else if (ev.key === "Escape" || ev.key === "Tab") { ev.preventDefault(); ev.stopPropagation(); V.menuClose(); }
    });
    const r = button.getBoundingClientRect(), lr = layer.getBoundingClientRect();
    box.style.top = (r.bottom - lr.top + 4) + "px";
    box.style.right = Math.max(8, lr.right - r.right) + "px";
    layer.append(catcher, box);
    button.setAttribute("aria-expanded", "true");
    open = { box, catcher, button };
    rows[0] && rows[0].focus();
  };
})();
