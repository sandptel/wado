// wado bridge — a prompt as a bottom sheet. One job: show a title and some finger-sized
// buttons, call back with the one pressed, and go away.
//
// Built on the menu sheet's look (`.wado-menusheet`, `.menurow` in stage.css) rather than a new
// one, so every bottom sheet in wado reads the same. Each sheet has a `tag`; showing a tag that
// is already up replaces it, and `W.sheet.close(tag)` removes it from outside (an approval that
// was answered on another device, say).
//
//   W.sheet.ask(tag, title, [{ label, primary?, run }])
//   W.sheet.close(tag)

W.sheet = {
  open: {},

  ask(tag, title, buttons) {
    W.sheet.close(tag);
    const el = document.createElement("div");
    el.className = "wado-menusheet wado-prompt";
    const head = document.createElement("div");
    head.className = "sheettitle";
    head.textContent = title;
    const rows = buttons.map((b) => {
      const btn = document.createElement("button");
      btn.className = "menurow" + (b.primary ? " cancel" : "");
      btn.textContent = b.label;
      btn.addEventListener("click", () => {
        W.sheet.close(tag);
        try { if (b.run) b.run(); } catch (e) { if (W.rlog) W.rlog("sheet " + tag + " threw: " + e); }
      });
      return btn;
    });
    el.replaceChildren(head, ...rows);
    document.body.appendChild(el);
    W.sheet.open[tag] = el;
  },

  close(tag) {
    const el = W.sheet.open[tag];
    if (el) el.remove();
    delete W.sheet.open[tag];
  },
};
