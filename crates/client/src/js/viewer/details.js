// wado bridge — the viewer's details: where the file is, what it is, and what to do with it.
//
// A bottom sheet on a phone, pulled up from the viewer's footer (drag or tap) and dragged back
// down, over a scrim — never drawn over the content together with its controls: while it is up,
// the content and its controls are inert. On a wide screen (≥ 900 px) it is a panel beside the
// content instead, which shrinks to make room.
//
//   const d = V.details(view)    d.fill(entry)  d.add(title, rows)  d.open()  d.close()  d.toggle()  d.isOpen()
//
// `rows` are [label, value, extra?] — `extra` an element after the value (an "Open" link).

(() => {
  const F = W.files;
  const V = F.viewer;
  const h = F.h;
  const WIDE = "(min-width: 900px)";
  const reduced = () => document.documentElement.dataset.motion === "off" || matchMedia("(prefers-reduced-motion: reduce)").matches;

  V.details = (view) => {
    const meta = h("div", { class: "fvmeta" });
    const close = h("button", { class: "fvbtn", "aria-label": "Close details", onclick: () => api.close() }, V.ico("x"));
    const grab = h("div", { class: "fvgrab", "aria-hidden": "true" });
    const head = h("div", { class: "fvdhead" }, grab, h("b", { id: "fvdtitle" }, "Details"), close);
    const sheet = h("aside", { class: "fvdetails", role: "dialog", "aria-labelledby": "fvdtitle", inert: true }, head, meta);
    const scrim = h("div", { class: "fvdscrim", onclick: () => api.close() });
    view.append(scrim, sheet);
    let up = false;

    const wide = () => matchMedia(WIDE).matches;
    const content = () => view.querySelectorAll(".fvstage, .fvfoot, .fvnav");
    const set = (on) => {
      up = on;
      const side = wide();
      view.classList.toggle("detailed", on && side);
      sheet.classList.toggle("side", side);
      sheet.classList.toggle("up", on);
      scrim.classList.toggle("up", on && !side);
      sheet.inert = !on;
      // A sheet over the content: the content and its controls are out of reach until it goes.
      for (const n of content()) n.inert = on && !side;
      if (on) (side ? sheet : close).focus({ preventScroll: true });
    };

    // Drag the sheet down to put it away: it follows the finger 1:1, then settles on the spring.
    let y0 = null, dy = 0;
    head.addEventListener("pointerdown", (ev) => {
      if (wide() || ev.target.closest("button")) return;
      y0 = ev.clientY; dy = 0;
      head.setPointerCapture(ev.pointerId);
      sheet.classList.add("dragging");
    });
    head.addEventListener("pointermove", (ev) => {
      if (y0 === null) return;
      dy = Math.max(0, ev.clientY - y0);
      sheet.style.translate = `0 ${dy}px`;
    });
    const release = () => {
      if (y0 === null) return;
      y0 = null;
      sheet.classList.remove("dragging");
      sheet.style.translate = "";
      if (dy > 80) api.close();
    };
    head.addEventListener("pointerup", release);
    head.addEventListener("pointercancel", release);

    const api = {
      el: sheet,
      isOpen: () => up,
      open: () => set(true),
      close: () => { if (up) { set(false); const b = view.querySelector(".fvdbtn"); if (b && !reduced()) b.focus({ preventScroll: true }); } },
      toggle: () => (up ? api.close() : api.open()),
      // Re-place it when the window crosses the wide breakpoint.
      relayout: () => { if (up) set(true); else sheet.classList.toggle("side", wide()); },
      fill(e) {
        const A = F.act || {};
        const folder = e.path.slice(0, e.path.lastIndexOf("/")) || "/";
        const home = F.info && F.info.home;
        const where = home && folder.startsWith(home) ? "~" + folder.slice(home.length) : folder;
        const act = (icon, label, run) => h("button", { class: "fvact", onclick: run }, V.ico(icon), h("span", {}, label));
        meta.replaceChildren(
          h("b", { class: "fvname" }, e.name),
          h("div", { class: "fvacts" },
            act("download", "Download", () => { F.download(e.path, e.name, false); F.toast("Downloading " + e.name); }),
            A.canShare && A.canShare(e) ? act("share", "Share", () => A.share(e)) : null,
            A.copyPath ? act("copy", "Copy path", () => A.copyPath(e.path)) : null,
            A.canOpenThere && A.canOpenThere() ? act("monitor", "On computer", () => A.openThere(e.path)) : null));
        api.add("", [
          ["Folder", where, A.showIn ? h("button", { class: "fvlink", onclick: () => A.showIn(e.path) }, "Open") : null],
          ...V.facts(e).map(([k, v]) => k === "Size" ? [k, `${F.fmtSize(e.size)} (${(e.size || 0).toLocaleString()} bytes)`] : [k, v]),
        ]);
      },
      add(title, rows) {
        rows = rows.filter((r) => r && r[1]);
        if (!rows.length) return;
        meta.append(h("div", { class: "fvcard" },
          title ? h("b", { class: "fvcardhead" }, title) : null,
          ...rows.map(([k, v, extra]) => h("div", { class: "fvrowi" }, h("small", {}, k), h("span", {}, v, extra || null)))));
      },
    };
    return api;
  };
})();
