// wado bridge — the file manager's window (Decision Log 2026-10-04, item 12).
//
// One overlay, two layouts by width. Under 600 px it is Google Files: a quick-access home
// (folders and recent files), tap to open, long-press to select, a floating button to add.
// Wider it is Nautilus: a sidebar of places, breadcrumbs, list or grid with sortable columns,
// click / Ctrl / Shift to select, double-click to open, drag and drop to upload.
//
// This file owns `#wado-files` outright — Dioxus only calls `filesOpen()` — because a progress
// bar that ticks sixty times a second must not re-render the whole app (the console's xterm
// is owned by js/pty.js for the same reason).
//
// Transfers, dialogs and the pill are js/files_panels.js; the wire is js/files_link.js.

(() => {
  const F = W.files;
  const S = (F.ui = {
    view: "home", path: null, entries: [], loading: false, err: "",
    sel: new Set(), selMode: false, anchor: null,
    sort: { by: "name", desc: false }, grid: false,
    quick: { pins: [], recent: [] }, picker: null, more: 1500,
  });
  try { Object.assign(S, JSON.parse(localStorage.getItem("wado.files.view")) || {}); } catch (_) {}
  S.sel = new Set();
  const keepView = () => { try { localStorage.setItem("wado.files.view", JSON.stringify({ sort: S.sort, grid: S.grid, path: S.path })); } catch (_) {} };

  // ── tiny DOM helper ──────────────────────────────────────────────────────────
  const h = (F.h = (tag, attrs = {}, ...kids) => {
    const el = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs || {})) {
      if (v === undefined || v === null || v === false) continue;
      if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
      else if (k === "html") el.innerHTML = v;
      else if (k === "class") el.className = v;
      else el.setAttribute(k, v === true ? "" : v);
    }
    for (const c of kids.flat()) if (c !== null && c !== undefined && c !== false) el.append(c.nodeType ? c : String(c));
    return el;
  });
  const ico = (n) => h("span", { class: "fi", html: F.icon(n) });
  const btn = (icon, label, run, cls = "") =>
    h("button", { class: "fbtn " + cls, "aria-label": label, title: label, onclick: (e) => { e.stopPropagation(); run(e); } }, ico(icon));

  F.fmtSize = (n) => {
    if (!n) return "0 B";
    const u = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.min(u.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
    return (n / 1024 ** i).toFixed(i && n / 1024 ** i < 10 ? 1 : 0) + " " + u[i];
  };
  F.fmtTime = (s) => {
    if (!s) return "";
    const d = new Date(s * 1000), now = new Date();
    if (d.toDateString() === now.toDateString()) return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    return d.toLocaleDateString([], { day: "numeric", month: "short", year: d.getFullYear() === now.getFullYear() ? undefined : "numeric" });
  };
  const wide = () => matchMedia("(min-width: 600px)").matches;
  const rw = () => F.info && F.info.access === "rw";
  const join = (dir, name) => (dir === "/" ? "" : dir.replace(/\/$/, "")) + "/" + name;
  const parentOf = (p) => p.replace(/\/[^/]*$/, "") || "/";
  const rootOf = (p) => (F.info ? F.info.roots.filter((r) => p === r || p.startsWith(r === "/" ? "/" : r + "/")).sort((a, b) => b.length - a.length)[0] : null);
  const label = (p) => {
    if (!F.info) return p;
    if (p === F.info.home) return "Home";
    if (p === "/") return "Computer";
    return p.split("/").pop();
  };

  // ── the overlay ──────────────────────────────────────────────────────────────
  let root = null;
  function mount() {
    if (root) return;
    root = h("div", { id: "wado-files", class: "files", role: "dialog", "aria-label": "Files", hidden: true });
    document.body.appendChild(root);
    addEventListener("resize", () => { if (F.isOpen()) render(); });
    root.addEventListener("keydown", keys);
    root.addEventListener("dragover", (e) => { if (rw() && S.view === "dir") { e.preventDefault(); root.classList.add("dropping"); } });
    root.addEventListener("dragleave", (e) => { if (e.target === root || !root.contains(e.relatedTarget)) root.classList.remove("dropping"); });
    root.addEventListener("drop", (e) => { e.preventDefault(); root.classList.remove("dropping"); if (rw() && S.view === "dir") dropped(e.dataTransfer); });
  }
  F.isOpen = () => !!root && !root.hidden;

  F.openUi = async (opts = {}) => {
    mount();
    if (root.hidden) {
      root.classList.add("opening");
      setTimeout(() => root.classList.remove("opening"), 600);
    }
    root.hidden = false;
    document.documentElement.classList.add("files-open");
    if (opts.transfers) F.showTransfers(true);
    // Already somewhere (a reopen): stay there.
    if (F.ready && S.placed) { render(); return; }
    S.loading = true; S.err = ""; render();
    try {
      await F.connect();
      S.placed = true;
      await loadQuick();
      const start = S.path && rootOf(S.path) ? S.path : null;
      if (start) await go(start);
      else if (wide()) await go(F.info.home && rootOf(F.info.home) ? F.info.home : F.info.roots[0]);
      else { S.view = "home"; S.loading = false; render(); }
    } catch (e) {
      S.loading = false;
      S.err = String(e.message || e);
      render();
    }
  };
  F.closeUi = () => {
    if (!root) return;
    root.hidden = true;
    document.documentElement.classList.remove("files-open");
    F.showTransfers(false);
    if (F.pill) F.pill();
  };

  async function loadQuick() {
    try { S.quick = await F.req("quick"); } catch (_) {}
  }

  async function go(path) {
    S.view = "dir"; S.path = path; S.loading = true; S.err = ""; S.sel.clear(); S.selMode = false; S.more = 1500;
    keepView();
    render();
    try {
      const r = await F.req("list", { path });
      if (S.path !== path) return;
      S.entries = r.entries;
    } catch (e) {
      if (S.path !== path) return;
      S.entries = [];
      S.err = String(e.message || e);
    }
    S.loading = false;
    render();
  }
  F.refresh = () => (S.view === "dir" ? go(S.path) : loadQuick().then(render));
  function showRecent() { S.view = "recent"; S.sel.clear(); S.selMode = false; S.err = ""; loadQuick().then(render); render(); }
  function home() { S.view = "home"; S.sel.clear(); S.selMode = false; S.err = ""; render(); }

  function back() {
    if (S.selMode || S.sel.size) { S.sel.clear(); S.selMode = false; return render(); }
    if (S.view === "dir" && S.path !== rootOf(S.path)) return go(parentOf(S.path));
    if (!wide() && S.view !== "home") return home();
    F.closeUi();
  }

  // The listing, sorted: folders first, then by the chosen column.
  function sorted() {
    const { by, desc } = S.sort;
    const k = (e) => (by === "size" ? e.size : by === "mtime" ? e.mtime : 0);
    const coll = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
    return [...S.entries].sort((a, b) => {
      if (a.dir !== b.dir) return a.dir ? -1 : 1;
      const c = by === "name" ? coll.compare(a.name, b.name) : k(a) - k(b) || coll.compare(a.name, b.name);
      return desc ? -c : c;
    });
  }

  // ── render ───────────────────────────────────────────────────────────────────
  function render() {
    if (!root || root.hidden) return;
    const isWide = wide();
    root.dataset.layout = isWide ? "wide" : "narrow";
    const panel = h("div", { class: "fpanel" + (S.picker ? " picking" : "") }, header(isWide), h("div", { class: "fbody" }, isWide ? sidebar() : null, main(isWide)));
    if (S.sel.size && !S.picker) panel.append(selbar());
    if (S.picker) panel.append(pickbar());
    if (!isWide && rw() && S.view === "dir" && !S.sel.size && !S.picker) panel.append(h("button", { class: "ffab", "aria-label": "Add", onclick: addSheet }, ico("plus")));
    // A re-render of the same folder (a selection, a sort) keeps the scroll position.
    const key = S.view + ":" + S.path;
    const top = root._key === key ? (root.querySelector(".fmain") || {}).scrollTop || 0 : 0;
    root.replaceChildren(panel, h("div", { class: "fdrop" }, ico("upload"), "Drop to upload here"));
    root._key = key;
    if (top) root.querySelector(".fmain").scrollTop = top;
    if (F.renderTransfers) F.renderTransfers(root);
  }
  F.render = render;

  function header(isWide) {
    const title = S.picker ? (S.picker.op === "move" ? "Move to…" : "Copy to…")
      : S.view === "home" ? "Files" : S.view === "recent" ? "Recent" : label(S.path || "");
    const crumbs = h("div", { class: "fcrumbs" });
    if (isWide && S.view === "dir" && S.path && rootOf(S.path)) {
      const r = rootOf(S.path);
      const parts = S.path.slice(r.length).split("/").filter(Boolean);
      let acc = r;
      crumbs.append(h("button", { class: "fcrumb", onclick: () => go(r) }, label(r)));
      for (const p of parts) {
        acc = join(acc, p);
        const to = acc;
        crumbs.append(h("span", { class: "fsep" }, "›"), h("button", { class: "fcrumb", onclick: () => go(to) }, p));
      }
    } else crumbs.append(h("b", { class: "ftitle" }, title));
    const n = F.active ? F.active().length : 0;
    return h("header", { class: "fhead" },
      btn("back", S.view === "home" || (isWide && S.view === "dir" && S.path === rootOf(S.path)) ? "Close" : "Back", back),
      crumbs,
      F.info && F.info.access === "ro" ? h("span", { class: "fchip" }, "read only") : null,
      isWide && S.view === "dir" ? btn(S.grid ? "list" : "grid", S.grid ? "List view" : "Grid view", () => { S.grid = !S.grid; keepView(); render(); }) : null,
      S.view === "dir" ? btn("sort", "Sort", sortSheet) : null,
      h("button", { class: "fbtn fxbtn" + (n ? " busy" : ""), "aria-label": "Transfers", title: "Transfers", onclick: () => F.showTransfers() }, ico("transfers"), n ? h("i", { class: "fbadge" }, n) : null),
      F.ready ? btn("dots", "More", moreSheet) : null,
      btn("x", "Close files", F.closeUi, "fclose"),
    );
  }

  function place(icon, name, run, on, extra) {
    return h("button", { class: "fplace" + (on ? " on" : ""), onclick: run }, ico(icon), h("span", {}, name), extra || null);
  }

  function sidebar() {
    const side = h("nav", { class: "fside" });
    if (!F.info) return side;
    side.append(place("clock", "Recent", showRecent, S.view === "recent"));
    for (const r of F.info.roots) side.append(place(r === F.info.home ? "home" : "drive", label(r), () => go(r), S.view === "dir" && S.path === r));
    if (S.quick.pins.length) side.append(h("div", { class: "fsidehead" }, "Pinned"));
    for (const p of S.quick.pins) {
      side.append(place(p.kind === "pin" ? "pin" : "folder", p.name, () => go(p.path), S.view === "dir" && S.path === p.path,
        p.kind === "pin" && rw() ? h("span", { class: "funpin", title: "Unpin", onclick: (e) => { e.stopPropagation(); pin(p.path, false); } }, "×") : null));
    }
    return side;
  }

  function main(isWide) {
    const m = h("main", { class: "fmain" });
    if (S.err && !F.ready) {
      m.append(h("div", { class: "fempty" }, ico("lock"), h("b", {}, "Files are not available"), h("p", {}, S.err),
        h("button", { class: "fgo", onclick: () => F.openUi() }, "Try again")));
      return m;
    }
    if (S.loading && !S.entries.length && S.view === "dir") { m.append(h("div", { class: "fempty" }, h("div", { class: "fspin" }))); return m; }
    if (!F.ready && S.loading) { m.append(h("div", { class: "fempty" }, h("div", { class: "fspin" }), h("p", {}, "Opening a secure channel to the computer…"))); return m; }
    if (W.e2eUnpinned && F.ready) m.append(h("p", { class: "fwarn" }, "This browser trusted the computer on first use. Scan its QR code once (wado qr) to pin it."));
    if (S.view === "home") return homeView(m);
    if (S.view === "recent") return fileList(m, S.quick.recent.map((r) => ({ ...r, dir: false, recent: true })), false);
    if (S.err) m.append(h("div", { class: "fempty" }, ico("lock"), h("p", {}, S.err)));
    else if (!S.entries.length) m.append(h("div", { class: "fempty" }, ico("folder"), h("p", {}, "This folder is empty")));
    else fileList(m, sorted(), isWide && S.grid);
    return m;
  }

  function homeView(m) {
    if (!F.info) return m;
    const chips = h("div", { class: "fchips" });
    for (const r of F.info.roots) chips.append(place(r === F.info.home ? "home" : "drive", label(r), () => go(r)));
    for (const p of S.quick.pins) chips.append(place(p.kind === "pin" ? "pin" : "folder", p.name, () => go(p.path)));
    m.append(h("div", { class: "fsec" }, "Folders"), chips);
    m.append(h("div", { class: "fsec" }, "Recent"));
    if (!S.quick.recent.length) m.append(h("p", { class: "fdim" }, "Nothing recent."));
    else fileList(m, S.quick.recent.slice(0, 20).map((r) => ({ ...r, dir: false, recent: true })), false);
    return m;
  }

  // ── the list (and grid) ───────────────────────────────────────────────────────
  let shown = []; // what the current list shows, for index → entry
  const pathOf = (e) => e.path || join(S.path, e.name);
  function fileList(m, list, grid) {
    shown = list.slice(0, S.more);
    const box = h("div", { class: grid ? "fgrid" : "flist", role: "listbox", "aria-multiselectable": "true" });
    if (!grid && wide() && S.view === "dir") {
      const col = (by, name) => h("button", { class: "fcol c-" + by + (S.sort.by === by ? " on" : ""), onclick: () => { S.sort = { by, desc: S.sort.by === by ? !S.sort.desc : by !== "name" }; keepView(); render(); } },
        name, S.sort.by === by ? (S.sort.desc ? " ↓" : " ↑") : "");
      box.append(h("div", { class: "frow fhd" }, h("span", {}), col("name", "Name"), col("size", "Size"), col("mtime", "Modified")));
    }
    shown.forEach((e, i) => {
      const k = F.kind(e.name, e.dir);
      const path = pathOf(e);
      const sel = S.sel.has(path);
      const disc = h("span", { class: "fdisc", style: `--hue:${k.hue}` }, ico(k.icon));
      if (k.image) thumb(disc, path, e.mtime);
      const meta = e.dir ? "Folder" : F.fmtSize(e.size);
      const row = grid
        ? h("div", { class: "ftile" + (sel ? " sel" : ""), "data-i": i, role: "option", "aria-selected": sel, tabindex: 0 }, disc, h("span", { class: "fname" }, e.name))
        : h("div", { class: "frow" + (sel ? " sel" : ""), "data-i": i, role: "option", "aria-selected": sel, tabindex: 0 },
          disc,
          h("span", { class: "fname" }, e.name, e.link ? h("i", { class: "flink" }, " ↗") : null,
            h("small", {}, e.recent ? parentOf(e.path).replace(F.info.home, "~") + " · " + F.fmtTime(e.mtime) : meta + " · " + F.fmtTime(e.mtime))),
          h("span", { class: "c-size" }, e.dir ? "—" : F.fmtSize(e.size)),
          h("span", { class: "c-mtime" }, F.fmtTime(e.mtime)),
          S.picker ? null : h("button", { class: "fbtn fmore", "aria-label": "Actions for " + e.name, "data-more": i }, ico("dots")));
      box.append(row);
    });
    if (list.length > shown.length) box.append(h("button", { class: "fgo fshowmore", onclick: () => { S.more += 1500; render(); } }, `Show ${list.length - shown.length} more`));
    wire(box);
    m.append(box);
    return m;
  }

  // Click / tap / long-press, delegated.
  let lastPointer = "mouse", pressTimer = null, pressed = false;
  function wire(box) {
    box.addEventListener("pointerdown", (ev) => {
      lastPointer = ev.pointerType;
      const row = ev.target.closest("[data-i]");
      if (!row || ev.pointerType === "mouse") return;
      pressed = false;
      clearTimeout(pressTimer);
      pressTimer = setTimeout(() => {
        pressed = true;
        if (S.picker) return;
        S.selMode = true;
        toggle(shown[+row.dataset.i]);
        if (navigator.vibrate) navigator.vibrate(15);
      }, 450);
    });
    const cancel = () => clearTimeout(pressTimer);
    box.addEventListener("pointerup", cancel);
    box.addEventListener("pointercancel", cancel);
    box.addEventListener("pointermove", (ev) => { if (ev.pointerType !== "mouse" && Math.abs(ev.movementY) > 4) cancel(); });
    box.addEventListener("contextmenu", (ev) => { if (lastPointer !== "mouse") ev.preventDefault(); });
    box.addEventListener("click", (ev) => {
      if (pressed) { pressed = false; return; }
      const more = ev.target.closest("[data-more]");
      if (more) { itemSheet(shown[+more.dataset.more]); return; }
      const row = ev.target.closest("[data-i]");
      if (!row) return;
      const e = shown[+row.dataset.i];
      if (S.picker) { if (e.dir) go(pathOf(e)); return; }
      const touch = lastPointer !== "mouse";
      if (touch || !wide()) {
        if (S.selMode || S.sel.size) return toggle(e);
        return activate(e);
      }
      // Desktop: select; Ctrl toggles, Shift extends.
      const path = pathOf(e);
      if (ev.shiftKey && S.anchor !== null) {
        const a = shown.findIndex((x) => pathOf(x) === S.anchor), b = +row.dataset.i;
        S.sel.clear();
        for (let i = Math.min(a, b); i <= Math.max(a, b); i++) S.sel.add(pathOf(shown[i]));
      } else if (ev.ctrlKey || ev.metaKey) toggle(e, true);
      else { S.sel.clear(); S.sel.add(path); S.anchor = path; }
      render();
    });
    box.addEventListener("dblclick", (ev) => {
      const row = ev.target.closest("[data-i]");
      if (row && lastPointer === "mouse" && !S.picker) activate(shown[+row.dataset.i]);
    });
  }
  function toggle(e, quiet) {
    const p = pathOf(e);
    if (S.sel.has(p)) S.sel.delete(p); else S.sel.add(p);
    S.anchor = p;
    if (!S.sel.size) S.selMode = false;
    if (!quiet) render();
  }
  function activate(e) {
    if (e.dir) return go(pathOf(e));
    if (lastPointer === "mouse" && wide()) return F.download(pathOf(e), e.name, false);
    itemSheet(e);
  }

  // Thumbnails: lazily, as rows come into view; cached by path and mtime.
  const thumbs = new Map();
  const io = "IntersectionObserver" in window ? new IntersectionObserver((xs) => {
    for (const x of xs) {
      if (!x.isIntersecting) continue;
      io.unobserve(x.target);
      const { path, mtime } = x.target._thumb;
      const key = path + "@" + mtime;
      if (!thumbs.has(key)) thumbs.set(key, F.req("thumb", { path }).then((r) => "data:image/png;base64," + r.png).catch(() => null));
      thumbs.get(key).then((url) => { if (url) { x.target.style.backgroundImage = `url("${url}")`; x.target.classList.add("thumb"); } });
    }
  }, { rootMargin: "200px" }) : null;
  function thumb(disc, path, mtime) {
    if (!io) return;
    disc._thumb = { path, mtime };
    io.observe(disc);
  }

  // ── selection bar, picker bar ────────────────────────────────────────────────
  const selected = () => [...S.sel];
  const entryOf = (p) => S.entries.find((e) => join(S.path, e.name) === p) || S.quick.recent.find((r) => r.path === p) || { name: p.split("/").pop(), dir: false };
  function selbar() {
    const n = S.sel.size;
    const one = n === 1 ? entryOf(selected()[0]) : null;
    return h("div", { class: "fselbar" },
      btn("x", "Clear selection", () => { S.sel.clear(); S.selMode = false; render(); }),
      h("b", {}, `${n} selected`),
      h("span", { class: "fgrow" }),
      btn("selectall", "Select all", () => { for (const e of shown) S.sel.add(pathOf(e)); render(); }),
      btn("download", "Download", () => { for (const p of selected()) { const e = entryOf(p); F.download(p, e.name, e.dir); } S.sel.clear(); S.selMode = false; render(); F.showTransfers(true); }),
      rw() ? btn("copy", "Copy to…", () => pick("copy", selected())) : null,
      rw() ? btn("move", "Move to…", () => pick("move", selected())) : null,
      rw() && one ? btn("edit", "Rename", () => rename(selected()[0])) : null,
      rw() ? btn("trash", "Move to Trash", () => trash(selected())) : null,
    );
  }
  function pick(op, paths) {
    S.picker = { op, paths, from: S.path };
    S.sel.clear(); S.selMode = false;
    if (S.view !== "dir") go(F.info.home); else render();
  }
  function pickbar() {
    const n = S.picker.paths.length;
    const verb = S.picker.op === "move" ? "Move" : "Copy";
    return h("div", { class: "fpickbar" },
      h("span", {}, `${verb} ${n} item${n > 1 ? "s" : ""} into `, h("b", {}, label(S.path || ""))),
      h("span", { class: "fgrow" }),
      h("button", { class: "fgo ghost", onclick: () => { S.picker = null; render(); } }, "Cancel"),
      h("button", { class: "fgo", disabled: S.view !== "dir", onclick: () => doPick("fail") }, verb + " here"),
    );
  }
  async function doPick(clash) {
    const { op, paths } = S.picker;
    try {
      await F.req(op, { paths, dest: S.path, clash });
      S.picker = null;
      F.toast(`${op === "move" ? "Moved" : "Copied"} ${paths.length} item${paths.length > 1 ? "s" : ""}`);
      go(S.path);
    } catch (e) {
      if (String(e.message).endsWith("exists")) {
        W.sheet.ask("files-clash", "Some names already exist here", [
          { label: "Keep both", run: () => doPick("rename") },
          { label: "Replace (the old ones go to the Trash)", run: () => doPick("replace") },
          { label: "Cancel", primary: true },
        ]);
      } else F.toast(String(e.message || e), true);
    }
  }

  // ── actions ──────────────────────────────────────────────────────────────────
  async function act(p, ok) {
    try { await p; if (ok) F.toast(ok); } catch (e) { F.toast(String(e.message || e), true); }
    F.refresh();
  }
  async function rename(path) {
    const old = path.split("/").pop();
    const to = await F.prompt("Rename", old, "Rename");
    if (to && to !== old) act(F.req("rename", { path, to }));
    S.sel.clear(); S.selMode = false;
  }
  function trash(paths) {
    const n = paths.length;
    W.sheet.ask("files-trash", n === 1 ? `Move “${paths[0].split("/").pop()}” to the Trash?` : `Move ${n} items to the Trash?`, [
      { label: "Move to Trash", run: () => { S.sel.clear(); S.selMode = false; act(F.req("trash", { paths }), n === 1 ? "Moved to the Trash" : `${n} items moved to the Trash`); } },
      { label: "Cancel", primary: true },
    ]);
  }
  async function newFolder() {
    const name = await F.prompt("New folder", "", "Create");
    if (name) act(F.req("mkdir", { path: join(S.path, name) }));
  }
  function pin(path, on) { act(F.req(on ? "pin" : "unpin", { path }), on ? "Pinned" : "Unpinned").then(() => loadQuick().then(render)); }

  function itemSheet(e) {
    const p = pathOf(e);
    const opts = [{ label: e.dir ? "Download as .zip" : "Download", run: () => { F.download(p, e.name, e.dir); F.showTransfers(true); } }];
    if (e.dir) opts.unshift({ label: "Open", run: () => go(p) });
    if (e.recent) opts.push({ label: "Show in folder", run: () => go(parentOf(p)) });
    if (rw()) {
      opts.push({ label: "Rename", run: () => rename(p) });
      opts.push({ label: "Copy to…", run: () => pick("copy", [p]) });
      opts.push({ label: "Move to…", run: () => pick("move", [p]) });
      if (e.dir) opts.push({ label: S.quick.pins.some((x) => x.path === p && x.kind === "pin") ? "Unpin" : "Pin to quick access", run: () => pin(p, !S.quick.pins.some((x) => x.path === p && x.kind === "pin")) });
      opts.push({ label: "Move to Trash", run: () => trash([p]) });
    }
    opts.push({ label: "Select", run: () => { S.selMode = true; toggle(e); } });
    opts.push({ label: "Cancel", primary: true });
    W.sheet.ask("files-item", `${e.name}${e.dir ? "" : " · " + F.fmtSize(e.size)}`, opts);
  }

  // Hidden pickers for uploads.
  function choose(folder) {
    const inp = h("input", { type: "file", multiple: true, style: "display:none" });
    if (folder) inp.webkitdirectory = true;
    inp.addEventListener("change", () => { if (inp.files.length) { F.upload(S.path, [...inp.files]); F.showTransfers(true); } inp.remove(); });
    document.body.appendChild(inp);
    inp.click();
  }
  function addSheet() {
    W.sheet.ask("files-add", "Add to " + label(S.path), [
      { label: "Upload files", run: () => choose(false) },
      { label: "Upload a folder", run: () => choose(true) },
      { label: "New folder", run: newFolder },
      { label: "Cancel", primary: true },
    ]);
  }
  function moreSheet() {
    const opts = [];
    if (S.view === "dir" && rw()) {
      opts.push({ label: "Upload files", run: () => choose(false) }, { label: "Upload a folder", run: () => choose(true) }, { label: "New folder", run: newFolder });
      const pinned = S.quick.pins.some((x) => x.path === S.path && x.kind === "pin");
      opts.push({ label: pinned ? "Unpin this folder" : "Pin this folder", run: () => pin(S.path, !pinned) });
    }
    if (S.view === "dir") opts.push({ label: "Download this folder (.zip)", run: () => { F.download(S.path, label(S.path), true); F.showTransfers(true); } });
    if (!wide()) opts.push({ label: S.grid ? "List view" : "Grid view", run: () => { S.grid = !S.grid; keepView(); render(); } });
    opts.push({ label: "Refresh", run: F.refresh });
    if (rw()) opts.push({ label: "Device access…", run: () => F.devicesPanel() });
    opts.push({ label: "Cancel", primary: true });
    W.sheet.ask("files-more", label(S.path || "Files"), opts);
  }
  function sortSheet() {
    const set = (by, desc) => () => { S.sort = { by, desc }; keepView(); render(); };
    W.sheet.ask("files-sort", "Sort by", [
      { label: "Name (A–Z)", run: set("name", false) },
      { label: "Name (Z–A)", run: set("name", true) },
      { label: "Newest first", run: set("mtime", true) },
      { label: "Oldest first", run: set("mtime", false) },
      { label: "Largest first", run: set("size", true) },
      { label: "Smallest first", run: set("size", false) },
    ]);
  }

  // Dropped files and folders, walked with their relative paths.
  async function dropped(dt) {
    const out = [];
    const walk = async (entry, prefix) => {
      if (entry.isFile) {
        const f = await new Promise((ok, no) => entry.file(ok, no));
        f.wadoPath = prefix + f.name;
        out.push(f);
      } else if (entry.isDirectory) {
        const r = entry.createReader();
        for (;;) {
          const batch = await new Promise((ok, no) => r.readEntries(ok, no));
          if (!batch.length) break;
          for (const c of batch) await walk(c, prefix + entry.name + "/");
        }
      }
    };
    const entries = [...dt.items].map((i) => i.webkitGetAsEntry && i.webkitGetAsEntry()).filter(Boolean);
    if (entries.length) for (const e of entries) await walk(e, "");
    else out.push(...dt.files);
    if (out.length) { F.upload(S.path, out); F.showTransfers(true); }
  }

  function keys(e) {
    if (e.target.closest("input, textarea")) return;
    if (e.key === "Escape") { e.preventDefault(); if (S.picker) { S.picker = null; render(); } else back(); return; }
    if (S.view !== "dir" || S.picker) return;
    if (e.key === "Backspace" || (e.altKey && e.key === "ArrowUp")) { e.preventDefault(); if (S.path !== rootOf(S.path)) go(parentOf(S.path)); }
    else if ((e.ctrlKey || e.metaKey) && e.key === "a") { e.preventDefault(); for (const x of shown) S.sel.add(pathOf(x)); render(); }
    else if (e.key === "Delete" && S.sel.size && rw()) trash(selected());
    else if (e.key === "F2" && S.sel.size === 1 && rw()) rename(selected()[0]);
    else if (e.key === "Enter" && S.sel.size === 1) activate(entryOf(selected()[0]));
  }

  F.uploaded = (x) => {
    clearTimeout(F._upRefresh);
    if (S.view === "dir" && x.dir === S.path) F._upRefresh = setTimeout(() => go(S.path), 400);
  };
  // The header's transfer count, kept current without re-rendering the listing.
  let lastN = -1;
  F.onXfer(() => {
    if (!F.isOpen()) return;
    const n = F.active().length;
    if (n === lastN) return;
    lastN = n;
    const b = root.querySelector(".fxbtn");
    if (!b) return;
    b.classList.toggle("busy", !!n);
    let i = b.querySelector(".fbadge");
    if (!n) { if (i) i.remove(); return; }
    if (!i) b.append((i = h("i", { class: "fbadge" })));
    i.textContent = n;
  });
  F.onDown(() => { if (F.isOpen() && !F.active().length) { S.err = "the connection to the computer was lost"; } });

  // Another device of this computer changed its files: a toast, and a fresh listing.
  W.relayOn("files_note", (m) => {
    const what = { upload: "uploaded", trash: "moved to the Trash", rename: "renamed", move: "moved", copy: "copied", mkdir: "created", download: "downloaded", "download-zip": "downloaded", pin: "pinned", unpin: "unpinned", grant: "changed file access:" }[m.op] || m.op;
    emit({ type: "notification", id: 0x7f000000 + Math.floor(Math.random() * 0xffffff), app: "Files", summary: `${m.device} ${what}`, body: m.path });
    if (F.isOpen() && S.view === "dir" && m.op !== "download" && m.op !== "download-zip") F.refresh();
  });

  W.filesOpen = () => F.openUi();
  W.filesClose = () => F.closeUi();
})();
