// wado bridge — native menu sheets (M-P S7).
//
// When an app opens a menu, the server reads its items from the accessibility tree and sends
// them here (RelayMsg::Menu); this draws them as a bottom sheet of finger-sized rows. Picking a
// row activates that item through the tree (MenuActivate) — no aiming at the app's 20-pixel
// rows. Cancel is Back: the compositor answers a Back with a popup open by sending Escape, and
// the app closes its own menu, which in turn closes this sheet.
//
// An app with no tree gets the lens over its real popup instead (tree: false). The sheet is
// state-driven: it shows whatever the latest Menu message says and hides on `null`.

W.menuSheet = {
  el: null,

  show(menu) {
    if (!menu) { W.menuSheet.hide(); return; }
    if (!menu.tree) {
      W.menuSheet.hide();
      // No tree: magnify the app's own popup, centred.
      const k = W.videoEl && W.targets.content(W.videoEl);
      if (k && W.lens) W.lens.open(k.left + (menu.x + menu.w / 2) * k.w, k.top + (menu.y + menu.h / 2) * k.h);
      return;
    }
    const sheet = W.menuSheet.el || document.createElement("div");
    sheet.className = "wado-menusheet";
    const rows = menu.items.map((item) => {
      const b = document.createElement("button");
      b.className = "menurow" + (item.checked ? " checked" : "");
      b.disabled = !item.enabled;
      b.textContent = item.name || "…";
      if (item.submenu) b.dataset.more = "›";
      b.addEventListener("click", () => {
        if (W.relayWs && W.relayWs.readyState === WebSocket.OPEN) {
          W.relayWs.send(JSON.stringify({ type: "menu_activate", id: item.id }));
        }
      });
      return b;
    });
    const cancel = document.createElement("button");
    cancel.className = "menurow cancel";
    cancel.textContent = "Cancel";
    cancel.addEventListener("click", () => W.windowAction("back"));
    sheet.replaceChildren(...rows, cancel);
    if (!W.menuSheet.el) {
      document.body.appendChild(sheet);
      W.menuSheet.el = sheet;
    }
  },

  hide() {
    if (W.menuSheet.el) W.menuSheet.el.remove();
    W.menuSheet.el = null;
  },
};

W.relayOn("menu", (msg) => W.menuSheet.show(msg.menu));
