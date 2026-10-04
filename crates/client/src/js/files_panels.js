// wado bridge — the file manager's smaller surfaces: the transfers drawer, the progress pill
// that stays on screen when the window is closed, a text prompt, a toast, and device access.
//
// The drawer and the pill redraw only themselves on every progress tick (files_xfer.js batches
// ticks to one per frame); the listing behind them is left alone.

(() => {
  const F = W.files;
  const h = F.h;
  const ico = (n) => h("span", { class: "fi", html: F.icon(n) });
  let drawer = false;

  F.showTransfers = (on) => {
    drawer = on === undefined ? !drawer : on;
    if (F.isOpen()) F.render();
    F.pill();
  };

  const eta = (x) => {
    if (!x.speed || !x.size) return "";
    const s = Math.max(0, (x.size - x.done) / x.speed);
    if (s < 60) return Math.ceil(s) + " s left";
    if (s < 3600) return Math.ceil(s / 60) + " min left";
    return (s / 3600).toFixed(1) + " h left";
  };
  function line(x) {
    const pct = x.size ? Math.min(100, (x.done / x.size) * 100) : 0;
    const say = {
      queued: "Waiting", asking: "Waiting for your answer", verifying: "Checking…", paused: "Paused", waiting: x.err,
      done: x.kind === "up" ? "Uploaded" : "Saved to Downloads", skipped: "Skipped", error: x.err,
    }[x.state];
    const what = x.state === "running"
      ? `${F.fmtSize(x.done)} of ${F.fmtSize(x.size)} · ${F.fmtSize(x.speed)}/s · ${eta(x)}`
      : say || "";
    const b = (icon, label, run) => h("button", { class: "fbtn", "aria-label": label, title: label, onclick: run }, ico(icon));
    return h("div", { class: "fxrow s-" + x.state },
      h("span", { class: "fdisc", style: `--hue:${x.kind === "up" ? "var(--base0B)" : "var(--base0D)"}` }, ico(x.kind === "up" ? "upload" : "download")),
      h("div", { class: "fxmid" },
        h("b", {}, x.name.split("/").pop()),
        h("div", { class: "fbar" }, h("i", { style: `width:${x.state === "done" ? 100 : pct}%` })),
        h("small", {}, what)),
      ["running", "queued", "waiting"].includes(x.state) ? b("pause", "Pause", () => F.pause(x)) : null,
      ["paused", "error"].includes(x.state) && !(x.kind === "up" && !x.file) ? b("play", "Resume", () => F.resume(x)) : null,
      x.state === "done" && x.kind !== "up" ? b("download", "Save again", () => F.saveAgain(x)) : null,
      b("x", x.state === "done" ? "Remove from the list" : "Cancel", () => F.cancel(x)),
    );
  }

  // Into the open window.
  F.renderTransfers = (root) => {
    const old = root.querySelector(".fxfers");
    if (!drawer) { if (old) old.remove(); return; }
    const list = F.xfers.slice().reverse();
    const box = h("section", { class: "fxfers", "aria-label": "Transfers" },
      h("div", { class: "fxhead" }, h("b", {}, "Transfers"), h("span", { class: "fgrow" }),
        list.some((x) => x.state === "done" || x.state === "skipped") ? h("button", { class: "fgo ghost", onclick: F.clearDone }, "Clear finished") : null,
        h("button", { class: "fbtn", "aria-label": "Close transfers", onclick: () => F.showTransfers(false) }, ico("down"))),
      list.length ? list.map(line) : h("p", { class: "fdim" }, "Nothing transferring. Downloads are kept on this device until they finish, so a dropped connection resumes."));
    if (old) old.replaceWith(box); else root.append(box);
  };

  // The pill: active transfers, shown while the window is closed. Tap opens the drawer.
  let pill = null;
  F.pill = () => {
    const act = F.active();
    const show = act.length && !F.isOpen();
    if (!show) { if (pill) pill.hidden = true; return; }
    if (!pill) {
      pill = h("button", { id: "wado-files-pill", "aria-label": "File transfers", onclick: () => F.openUi({ transfers: true }) });
      document.body.appendChild(pill);
    }
    const size = act.reduce((n, x) => n + (x.size || 0), 0), done = act.reduce((n, x) => n + x.done, 0);
    const up = act.some((x) => x.kind === "up"), down = act.some((x) => x.kind !== "up");
    pill.hidden = false;
    pill.replaceChildren(ico(up && !down ? "upload" : down && !up ? "download" : "swap"),
      h("span", {}, `${act.length} · ${size ? Math.floor((done / size) * 100) : 0}%`),
      h("i", { style: `width:${size ? (done / size) * 100 : 0}%` }));
  };
  F.onXfer(() => {
    F.pill();
    if (F.isOpen() && drawer) F.renderTransfers(document.getElementById("wado-files"));
  });

  // A text prompt over the window: resolves to the text, or null.
  F.prompt = (title, value, ok) => new Promise((done) => {
    const input = h("input", { class: "finput", value, "aria-label": title, autocapitalize: "off", spellcheck: "false" });
    const close = (v) => { box.remove(); done(v); };
    const box = h("div", { class: "fmodal", onclick: (e) => { if (e.target === box) close(null); } },
      h("form", { class: "fdialog", onsubmit: (e) => { e.preventDefault(); const v = input.value.trim(); close(v || null); } },
        h("b", {}, title), input,
        h("div", { class: "fdbtns" },
          h("button", { type: "button", class: "fgo ghost", onclick: () => close(null) }, "Cancel"),
          h("button", { type: "submit", class: "fgo" }, ok))));
    document.body.appendChild(box);
    input.focus();
    // Select the name without its extension, as Nautilus does.
    const dot = value.lastIndexOf(".");
    input.setSelectionRange(0, dot > 0 ? dot : value.length);
    input.addEventListener("keydown", (e) => { if (e.key === "Escape") { e.stopPropagation(); close(null); } });
  });

  F.toast = (text, bad) => {
    const t = h("div", { class: "ftoast" + (bad ? " bad" : ""), role: "status" }, text);
    document.body.appendChild(t);
    setTimeout(() => t.remove(), bad ? 5000 : 2500);
  };

  // Which devices may use files — for an `rw`, QR-paired device; the computer's own
  // `wado files grant` is the other way.
  F.devicesPanel = async () => {
    let list;
    try { list = (await F.req("devices")).devices; } catch (e) { return F.toast(String(e.message || e), true); }
    const box = h("div", { class: "fmodal", onclick: (e) => { if (e.target === box) box.remove(); } });
    const rows = list.map((d) => {
      const seg = h("div", { class: "fseg" }, ["none", "ro", "rw"].map((l) =>
        h("button", { class: d.files === l ? "on" : "", disabled: !d.pinned || d.me, onclick: async () => {
          try { await F.req("grant", { key: d.key, level: l }); d.files = l; box.remove(); F.devicesPanel(); }
          catch (e) { F.toast(String(e.message || e), true); }
        } }, { none: "None", ro: "Read", rw: "Read & write" }[l])));
      return h("div", { class: "fdev" },
        h("div", {}, h("b", {}, d.name || d.key.slice(0, 8), d.me ? " (this device)" : ""),
          h("small", {}, d.pinned ? "paired by QR" : "not paired by QR — no file access until it scans the code")),
        seg);
    });
    box.append(h("div", { class: "fdialog wide" }, h("b", {}, "Device access to files"), ...rows,
      h("div", { class: "fdbtns" }, h("button", { class: "fgo", onclick: () => box.remove() }, "Done"))));
    document.body.appendChild(box);
  };
})();
