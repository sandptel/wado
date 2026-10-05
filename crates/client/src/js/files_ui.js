// wado bridge — the file manager's window (Decision Log 2026-10-04, item 12; revised 2026-10-05).
//
// Who it is for: someone at their phone or another computer reaching the files of the computer
// wado runs on. So the first screen answers "where is that thing" — search, categories (Photos,
// Videos, …), recent files, pinned folders, how full the disk is — and every file offers what only
// this bridge can: open it *on the computer*, view it here, take it to this device, share it.
//
// One overlay, two layouts by width. Under 600 px it is Google Files: an overview to start from,
// tap to open, long-press to select, a floating "New" button. Wider it is Nautilus: a sidebar of
// places, breadcrumbs, sortable columns, click / Ctrl / Shift to select, double-click to open,
// right-click for actions, drag and drop to upload. List or grid, one tap, in both.
//
// Views: `home` (the overview), `dir` (a folder), `recent`, `results` (a search or a category).
//
// This file owns `#wado-files` outright — Dioxus only calls `filesOpen()` — because a progress
// bar that ticks sixty times a second must not re-render the whole app. Transfers, dialogs and
// the pill are js/files_panels.js; the wire is js/files_link.js; the viewer js/files_view.js.

(() => {
  const F = W.files;
  const SAVED = "wado.files.view";
  const S = (F.ui = {
    view: "home", path: null, entries: [], loading: false, err: "",
    sel: new Set(), selMode: false, anchor: null,
    sort: { by: "name", desc: false }, grid: false, hidden: false,
    quick: { pins: [], recent: [] }, picker: null, more: 1500,
    filter: "", searching: false, results: null,
  });
  try { const v = JSON.parse(localStorage.getItem(SAVED)) || {}; for (const k of ["sort", "grid", "hidden", "path"]) if (k in v) S[k] = v[k]; } catch (_) {}
  const keepView = () => { try { localStorage.setItem(SAVED, JSON.stringify({ sort: S.sort, grid: S.grid, hidden: S.hidden, path: S.path })); } catch (_) {} };

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
  // A key typed into a text field — not a shortcut. A key event's target can be the document
  // itself (no `closest`), so this is the one place that asks.
  F.inField = (e) => !!(e.target && e.target.closest && e.target.closest("input, textarea, [contenteditable]"));
  const wide = () => matchMedia("(min-width: 600px)").matches;
  const touchy = () => matchMedia("(pointer: coarse)").matches;
  const rw = () => F.info && F.info.access === "rw";
  const join = (dir, name) => (dir === "/" ? "" : dir.replace(/\/$/, "")) + "/" + name;
  const parentOf = (p) => p.replace(/\/[^/]*$/, "") || "/";
  const rootOf = (p) => (F.info && p ? F.info.roots.filter((r) => p === r || p.startsWith(r === "/" ? "/" : r + "/")).sort((a, b) => b.length - a.length)[0] : null);
  const inTrash = (p) => !!(F.info && F.info.trash && p && (p === F.info.trash || p.startsWith(F.info.trash + "/")));
  const label = (p) => {
    if (!F.info || !p) return p || "";
    if (p === F.info.home) return "Home";
    if (p === "/") return "Computer";
    if (p === F.info.trash) return "Trash";
    return p.split("/").pop();
  };
  const tidy = (p) => (F.info && F.info.home && p.startsWith(F.info.home) ? "~" + p.slice(F.info.home.length) : p);

  // What only this bridge can do: open the file in the running session, on the computer.
  // `sh -c` runs session launches, so the path is single-quoted.
  const quote = (p) => "'" + p.replace(/'/g, "'\\''") + "'";
  const canOpenThere = () => !!W.sessionOn;
  function openThere(p) {
    if (W.relaySendMsg({ type: "session_launch", command: "xdg-open " + quote(p) })) F.toast("Opening on the computer…");
    else F.toast("Not connected to the computer", true);
  }
  // Hand a file to another app on this device (share sheet). Small files only: it travels whole.
  const SHARE_MAX = 64 << 20;
  const canShare = (e) => !e.dir && e.size <= SHARE_MAX && !!navigator.canShare && navigator.canShare({ files: [new File([""], "x.txt", { type: "text/plain" })] });
  async function share(e) {
    try {
      F.toast("Preparing " + e.name + "…");
      const v = await F.fetchFile({ ...e, path: pathOf(e) });
      await navigator.share({ files: [new File([v.blob], e.name, { type: v.blob.type || "application/octet-stream" })] });
    } catch (err) { if (err && err.name !== "AbortError") F.toast(String(err.message || err), true); }
  }
  function copyPath(p) {
    (navigator.clipboard ? navigator.clipboard.writeText(p) : Promise.reject(new Error("no clipboard"))).then(() => F.toast("Path copied"), () => F.toast(p));
  }

  // For the viewer's details panel: the same actions the item sheet offers.
  F.act = {
    openThere, canOpenThere, share, canShare, copyPath,
    showIn: (p) => { if (F.viewClose) F.viewClose(); go(parentOf(p)); },
  };

  // ── the overlay ──────────────────────────────────────────────────────────────
  let root = null;
  function mount() {
    if (root) return;
    root = h("div", { id: "wado-files", class: "files", role: "dialog", "aria-label": "Files", hidden: true });
    document.body.appendChild(root);
    let lastWide = wide();
    addEventListener("resize", () => { if (F.isOpen() && wide() !== lastWide) { lastWide = wide(); render(); } });
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
    if (F.viewClose) F.viewClose();
    if (F.viewerForget) F.viewerForget();
    F.showTransfers(false);
    if (F.pill) F.pill();
  };

  async function loadQuick() {
    try { S.quick = await F.req("quick"); } catch (_) {}
  }
  const reset = () => { S.sel.clear(); S.selMode = false; S.err = ""; S.more = 1500; S.filter = ""; S.searching = false; };

  async function go(path) {
    reset();
    S.view = "dir"; S.path = path; S.loading = true;
    keepView();
    render();
    try {
      const r = await F.req("list", { path });
      if (S.path !== path || S.view !== "dir") return;
      S.entries = r.entries;
    } catch (e) {
      if (S.path !== path) return;
      S.entries = [];
      S.err = String(e.message || e);
    }
    S.loading = false;
    render();
  }
  F.refresh = () => (S.view === "dir" ? go(S.path) : S.view === "results" && S.results ? search(S.results.q) : loadQuick().then(render));
  function showRecent() { reset(); S.view = "recent"; loadQuick().then(render); render(); }
  function home() { reset(); S.view = "home"; loadQuick().then(render); render(); }

  // A search or a category: `{ title, path, query, kind, grid }`.
  async function search(q) {
    reset();
    S.view = "results";
    S.results = { ...q, entries: [], loading: true, partial: false };
    render();
    try {
      const r = await F.req("find", { path: q.path || "", query: q.query || "", kind: q.kind || "" });
      if (S.view !== "results" || S.results.title !== q.title) return;
      Object.assign(S.results, { entries: r.entries, partial: r.partial, loading: false });
    } catch (e) {
      Object.assign(S.results, { loading: false });
      S.err = String(e.message || e);
    }
    S.results.q = q;
    render();
  }

  function back() {
    if (S.selMode || S.sel.size) { S.sel.clear(); S.selMode = false; return render(); }
    if (S.searching) { S.searching = false; S.filter = ""; return render(); }
    if (S.view === "dir" && S.path !== rootOf(S.path) && !(inTrash(S.path) && S.path === F.info.trash)) return go(parentOf(S.path));
    if (S.view === "results" && S.results && S.results.path && S.results.query) return go(S.results.path);
    if (S.view !== "home") return home();
    F.closeUi();
  }

  // The listing, sorted (folders first) and filtered by the search box and the hidden switch.
  function sorted(list) {
    const { by, desc } = S.sort;
    const k = (e) => (by === "size" ? e.size : by === "mtime" ? e.mtime : 0);
    const coll = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
    const f = S.filter.trim().toLowerCase();
    return list
      .filter((e) => (S.hidden || !e.name.startsWith(".")) && (!f || e.name.toLowerCase().includes(f)))
      .sort((a, b) => {
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
    const key = S.view + ":" + S.path + ":" + (S.results ? S.results.title : "");
    const top = root._key === key ? (root.querySelector(".fmain") || {}).scrollTop || 0 : 0;
    const panel = h("div", { class: "fpanel" + (S.picker ? " picking" : "") },
      header(isWide),
      h("div", { class: "fbody" }, isWide ? sidebar() : null, h("main", { class: "fmain" })));
    if (S.sel.size && !S.picker) panel.append(selbar());
    if (S.picker) panel.append(pickbar());
    if (rw() && S.view === "dir" && !S.sel.size && !S.picker && !inTrash(S.path)) {
      panel.append(h("button", { class: "ffab", "aria-label": "New", onclick: addSheet }, ico("plus"), h("span", {}, "New")));
    }
    root.replaceChildren(panel, h("div", { class: "fdrop" }, ico("upload"), "Drop to upload here"));
    renderMain();
    root._key = key;
    if (top) root.querySelector(".fmain").scrollTop = top;
    if (F.renderTransfers) F.renderTransfers(root);
  }
  F.render = render;
  // Only the main area — what typing in the filter box redraws, so the box keeps its focus.
  function renderMain() {
    const m = root.querySelector(".fmain");
    if (m) m.replaceChildren(...main(wide()));
  }

  function header(isWide) {
    const title = S.picker ? (S.picker.op === "move" ? "Move to…" : "Copy to…")
      : S.view === "home" ? "Files" : S.view === "recent" ? "Recent" : S.view === "results" ? (S.results ? S.results.title : "Search") : label(S.path);
    const n = F.active ? F.active().length : 0;
    const top = h("div", { class: "fhrow" },
      btn("back", S.view === "home" && !S.selMode ? "Close" : "Back", back),
      S.view === "dir" && S.path ? crumbs() : h("b", { class: "ftitle" }, title),
      F.info && F.info.access === "ro" ? h("span", { class: "fchip warn" }, "read only") : null,
      F.ready ? btn("search", "Search", () => { S.searching = !S.searching; S.filter = ""; render(); if (S.searching) setTimeout(() => root.querySelector(".fsearch input")?.focus(), 30); }, S.searching ? "on" : "") : null,
      S.view !== "home" ? btn(S.grid ? "list" : "grid", S.grid ? "List view" : "Grid view", () => { S.grid = !S.grid; keepView(); render(); }, "fviewbtn") : null,
      h("button", { class: "fbtn fxbtn" + (n ? " busy" : ""), "aria-label": "Transfers", title: "Transfers", onclick: () => F.showTransfers() }, ico("transfers"), n ? h("i", { class: "fbadge" }, n) : null),
      F.ready ? btn("dots", "More", moreSheet) : null,
      isWide ? btn("x", "Close files", F.closeUi, "fclose") : null,
    );
    const rows = [top];
    if (S.searching) rows.push(searchRow());
    if (S.view === "dir" && !S.loading && !S.err) rows.push(toolbar(S.entries));
    if (S.view === "results" && S.results && !S.results.loading) rows.push(toolbar(S.results.entries, true));
    return h("header", { class: "fhead" }, rows);
  }

  // The path as buttons — on a phone too, scrolled to its end.
  function crumbs() {
    const c = h("div", { class: "fcrumbs" });
    const r = inTrash(S.path) ? F.info.trash : rootOf(S.path);
    if (!r) return h("b", { class: "ftitle" }, label(S.path));
    const parts = S.path.slice(r.length).split("/").filter(Boolean);
    let acc = r;
    c.append(h("button", { class: "fcrumb", onclick: () => go(r) }, label(r)));
    for (const p of parts) {
      acc = join(acc, p);
      const to = acc;
      c.append(h("span", { class: "fsep" }, "›"), h("button", { class: "fcrumb", onclick: () => go(to) }, p));
    }
    requestAnimationFrame(() => { c.scrollLeft = c.scrollWidth; });
    return c;
  }

  function searchRow() {
    const everywhere = S.view === "home" || S.view === "recent";
    const input = h("input", {
      type: "search", value: S.filter, enterkeyhint: "search", autocapitalize: "off", spellcheck: "false",
      placeholder: everywhere ? "Search all files on the computer" : "Filter · Enter searches subfolders",
      oninput: (e) => { S.filter = e.target.value; if (!everywhere) renderMain(); },
      onkeydown: (e) => {
        if (e.key === "Enter" && S.filter.trim()) { e.preventDefault(); deepSearch(everywhere); }
        if (e.key === "Escape") { e.stopPropagation(); S.searching = false; S.filter = ""; render(); }
      },
    });
    return h("div", { class: "fsearch" }, ico("search"), input,
      h("button", { class: "fgo slim", onclick: () => { if (S.filter.trim()) deepSearch(everywhere); } }, everywhere ? "Search" : "In subfolders"));
  }
  function deepSearch(everywhere) {
    const q = S.filter.trim();
    const where = everywhere ? "" : S.path;
    search({ title: `“${q}”${where ? " in " + label(where) : ""}`, path: where, query: q });
  }

  // Sort chip, hidden-files chip, and what is here.
  function toolbar(list, results) {
    const shownList = sorted(list);
    const dirs = shownList.filter((e) => e.dir).length, files = shownList.length - dirs;
    const names = { name: "Name", mtime: "Modified", size: "Size" };
    const sum = [dirs ? `${dirs} folder${dirs > 1 ? "s" : ""}` : "", files ? `${files} file${files > 1 ? "s" : ""}` : ""].filter(Boolean).join(" · ") || "empty";
    return h("div", { class: "ftools" },
      results ? null : h("button", { class: "fchip", onclick: sortSheet }, ico("sort"), `${names[S.sort.by]} ${S.sort.desc ? "↓" : "↑"}`),
      results ? null : h("button", { class: "fchip" + (S.hidden ? " on" : ""), onclick: () => { S.hidden = !S.hidden; keepView(); render(); } }, ico("eye"), "Hidden"),
      inTrash(S.path) && !results ? h("span", { class: "fchip warn" }, "Trash · restore from the ⋯ of an item") : null,
      h("span", { class: "fgrow" }),
      h("small", { class: "fsum" }, sum + (results && S.results.partial ? " · more exist, narrow the search" : "")),
    );
  }

  function place(icon, name, run, on, extra, hue) {
    return h("button", { class: "fplace" + (on ? " on" : ""), onclick: run, style: hue ? `--hue:${hue}` : null }, ico(icon), h("span", {}, name), extra || null);
  }

  function sidebar() {
    const side = h("nav", { class: "fside" });
    if (!F.info) return side;
    const at = (p) => S.view === "dir" && S.path === p;
    side.append(
      place("grid", "Overview", home, S.view === "home"),
      place("clock", "Recent", showRecent, S.view === "recent"),
      h("div", { class: "fsidehead" }, "This computer"));
    for (const r of F.info.roots) side.append(place(r === F.info.home ? "home" : "drive", label(r), () => go(r), at(r)));
    if (S.quick.pins.length) side.append(h("div", { class: "fsidehead" }, "Pinned"));
    for (const p of S.quick.pins) {
      // Unpin lives in the folder's ⋯ menu — an inline × here was too easy to misclick.
      side.append(place(p.kind === "pin" ? "pin" : "folder", p.name, () => go(p.path), at(p.path)));
    }
    if (F.info.trash) side.append(h("div", { class: "fsidehead" }, ""), place("trash", "Trash", () => go(F.info.trash), inTrash(S.path) && S.view === "dir"));
    side.append(h("span", { class: "fgrow" }), ...storage(true));
    return side;
  }

  // The disk meter of each root.
  function storage(compact) {
    return (F.info && F.info.space || []).map((s) => {
      const used = s.total - s.free, pct = s.total ? Math.round((used / s.total) * 100) : 0;
      return h("div", { class: "fstore" + (compact ? " compact" : "") + (pct > 90 ? " full" : "") },
        h("div", { class: "fstorehd" }, ico("drive"), h("b", {}, label(s.root)), h("small", {}, `${F.fmtSize(s.free)} free`)),
        h("div", { class: "fbar" }, h("i", { style: `width:${pct}%` })),
        compact ? null : h("small", { class: "fdim0" }, `${F.fmtSize(used)} of ${F.fmtSize(s.total)} used`));
    });
  }

  function main(isWide) {
    const out = [];
    if (S.err && !F.ready) {
      return [h("div", { class: "fempty" }, ico("lock"), h("b", {}, "Files are not available"), h("p", {}, S.err),
        h("button", { class: "fgo", onclick: () => F.openUi() }, "Try again"))];
    }
    if (!F.ready && S.loading) return [h("div", { class: "fempty" }, h("div", { class: "fspin" }), h("p", {}, "Opening a secure channel to the computer…"))];
    if (W.e2eUnpinned && F.ready) out.push(h("p", { class: "fwarn" }, "This browser trusted the computer on first use. Scan its QR code once (wado qr) to pin it."));
    if (S.view === "home") return out.concat(homeView(isWide));
    if (S.view === "recent") return out.concat(fileList(sorted(S.quick.recent.map((r) => ({ ...r, dir: false, recent: true }))).sort((a, b) => b.mtime - a.mtime), S.grid));
    if (S.view === "results") {
      const R = S.results;
      if (!R || R.loading) return out.concat(h("div", { class: "fempty" }, h("div", { class: "fspin" }), h("p", {}, "Searching the computer…")));
      if (!R.entries.length) return out.concat(h("div", { class: "fempty" }, ico("search"), h("b", {}, "Nothing found"), h("p", {}, "Try another word, or search a wider folder.")));
      return out.concat(fileList(R.entries.map((e) => ({ ...e, recent: true })), S.grid || R.grid));
    }
    if (S.loading && !S.entries.length) return out.concat(h("div", { class: "fempty" }, h("div", { class: "fspin" })));
    if (S.err) return out.concat(h("div", { class: "fempty" }, ico("lock"), h("b", {}, "Can't open this folder"), h("p", {}, S.err)));
    const list = sorted(S.entries);
    if (!list.length) {
      return out.concat(h("div", { class: "fempty" }, ico(S.filter ? "search" : inTrash(S.path) ? "trash" : "folder"),
        h("b", {}, S.filter ? "No matches here" : inTrash(S.path) ? "The Trash is empty" : "This folder is empty"),
        S.filter ? h("button", { class: "fgo", onclick: () => deepSearch(false) }, "Search subfolders") : null,
        !S.filter && rw() && !inTrash(S.path) ? h("p", {}, isWide ? "Drop files here, or use New." : "Tap New to upload or make a folder.") : null));
    }
    return out.concat(fileList(list, S.grid));
  }

  // ── the overview ─────────────────────────────────────────────────────────────
  const CATS = [
    ["Photos", "image", "image", "var(--base0E)"], ["Videos", "video", "video", "var(--base08)"], ["Music", "audio", "audio", "var(--base0C)"],
    ["Documents", "doc", "doc", "var(--base0D)"], ["Archives", "archive", "archive", "var(--base09)"],
  ];
  function homeView(isWide) {
    if (!F.info) return [];
    const out = [];
    if (!S.searching) {
      out.push(h("button", { class: "fsearchfake", onclick: () => { S.searching = true; render(); setTimeout(() => root.querySelector(".fsearch input")?.focus(), 30); } },
        ico("search"), h("span", {}, "Search files on the computer")));
    }
    out.push(h("div", { class: "fsec" }, "Categories"));
    const cats = h("div", { class: "fcats" });
    for (const [name, kind, icon, hue] of CATS) {
      cats.append(h("button", { class: "fcat", style: `--hue:${hue}`, onclick: () => search({ title: name, kind, grid: kind === "image" || kind === "video" }) },
        h("span", { class: "fcatic" }, ico(icon)), h("b", {}, name)));
    }
    const dl = S.quick.pins.find((p) => /\/Downloads$/.test(p.path));
    if (dl) cats.append(h("button", { class: "fcat", style: "--hue:var(--base0B)", onclick: () => go(dl.path) }, h("span", { class: "fcatic" }, ico("download")), h("b", {}, "Downloads")));
    out.push(cats);

    const recent = S.quick.recent.slice(0, 16);
    if (recent.length) {
      out.push(h("div", { class: "fsec" }, "Recent", h("button", { class: "fseclink", onclick: showRecent }, "See all")));
      const strip = h("div", { class: "fstrip" });
      for (const r of recent) {
        const e = { ...r, dir: false, recent: true };
        const k = F.kind(e.name, false);
        const art = h("span", { class: "fart", style: `--hue:${k.hue}` }, ico(k.icon));
        if (k.image || k.icon === "video") thumb(art, e.path, e.mtime);
        strip.append(h("button", { class: "frcard", title: tidy(e.path), onclick: () => activate(e), oncontextmenu: (ev) => { ev.preventDefault(); itemSheet(e); } },
          art, h("b", {}, e.name), h("small", {}, F.fmtTime(e.mtime))));
      }
      out.push(strip);
    }

    out.push(h("div", { class: "fsec" }, "Folders"));
    const chips = h("div", { class: "fchips" });
    for (const r of F.info.roots) chips.append(place(r === F.info.home ? "home" : "drive", label(r), () => go(r)));
    for (const p of S.quick.pins) chips.append(place(p.kind === "pin" ? "pin" : "folder", p.name, () => go(p.path)));
    if (F.info.trash) chips.append(place("trash", "Trash", () => go(F.info.trash)));
    out.push(chips);
    if (!isWide) out.push(h("div", { class: "fsec" }, "Storage"), ...storage(false));
    return out;
  }

  // ── the list and the grid ────────────────────────────────────────────────────
  let shown = []; // what the current list shows, for index → entry
  const pathOf = (e) => e.path || join(S.path, e.name);
  function fileList(list, grid) {
    shown = list.slice(0, S.more);
    const box = h("div", { class: grid ? "fgrid" : "flist", role: "listbox", "aria-multiselectable": "true" });
    if (!grid && wide()) {
      const col = (by, name) => h("button", { class: "fcol c-" + by + (S.sort.by === by ? " on" : ""), onclick: () => { S.sort = { by, desc: S.sort.by === by ? !S.sort.desc : by !== "name" }; keepView(); render(); } },
        name, S.sort.by === by ? (S.sort.desc ? " ↓" : " ↑") : "");
      box.append(h("div", { class: "frow fhd" }, h("span", {}), col("name", "Name"), col("size", "Size"), col("mtime", "Modified"), h("span", {})));
    }
    const picking = S.selMode || S.sel.size > 1;
    shown.forEach((e, i) => {
      const k = F.kind(e.name, e.dir);
      const path = pathOf(e);
      const sel = S.sel.has(path);
      const where = e.recent ? tidy(parentOf(path)) : "";
      const meta = e.dir ? (where || "Folder") : [F.fmtSize(e.size), F.fmtTime(e.mtime), where].filter(Boolean).join(" · ");
      if (grid) {
        const art = h("span", { class: "fart", style: `--hue:${k.hue}` }, ico(k.icon));
        if (k.image || k.icon === "video") thumb(art, path, e.mtime);
        if (k.icon === "video") art.append(h("i", { class: "fplay" }, ico("play")));
        box.append(h("div", { class: "ftile" + (sel ? " sel" : ""), "data-i": i, role: "option", "aria-selected": sel, tabindex: 0, title: e.name },
          art,
          picking || sel ? h("span", { class: "fcheck" + (sel ? " on" : "") }, ico("check")) : null,
          h("span", { class: "fname" }, e.name, h("small", {}, e.dir ? "Folder" : F.fmtSize(e.size))),
          S.picker ? null : h("button", { class: "fbtn fmore", "aria-label": "Actions for " + e.name, "data-more": i }, ico("dots"))));
      } else {
        const disc = h("span", { class: "fdisc", style: `--hue:${k.hue}` }, sel && picking ? ico("check") : ico(k.icon));
        if ((k.image || k.icon === "video") && !(sel && picking)) thumb(disc, path, e.mtime);
        box.append(h("div", { class: "frow" + (sel ? " sel" : ""), "data-i": i, role: "option", "aria-selected": sel, tabindex: 0 },
          disc,
          h("span", { class: "fname" }, e.name, e.link ? h("i", { class: "flink" }, " ↗") : null, h("small", {}, meta)),
          h("span", { class: "c-size" }, e.dir ? "—" : F.fmtSize(e.size)),
          h("span", { class: "c-mtime" }, F.fmtTime(e.mtime)),
          S.picker ? h("span", {}) : h("button", { class: "fbtn fmore", "aria-label": "Actions for " + e.name, "data-more": i }, ico("dots"))));
      }
    });
    if (list.length > shown.length) box.append(h("button", { class: "fgo fshowmore", onclick: () => { S.more += 1500; render(); } }, `Show ${list.length - shown.length} more`));
    wire(box);
    return [box];
  }

  // Click / tap / long-press / right-click, delegated.
  let lastPointer = "mouse", pressTimer = null, pressed = false;
  function wire(box) {
    box.addEventListener("pointerdown", (ev) => {
      lastPointer = ev.pointerType;
      const row = ev.target.closest("[data-i]");
      if (!row || ev.pointerType === "mouse" || ev.target.closest("[data-more]")) return;
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
    box.addEventListener("contextmenu", (ev) => {
      ev.preventDefault();
      const row = ev.target.closest("[data-i]");
      if (!row || lastPointer !== "mouse" || S.picker) return;
      const e = shown[+row.dataset.i];
      if (!S.sel.has(pathOf(e))) { S.sel.clear(); S.sel.add(pathOf(e)); render(); }
      if (S.sel.size > 1) return;
      itemSheet(e);
    });
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
    if (F.viewable(e.name)) return openViewer(e);
    itemSheet(e);
  }
  // The viewer, with the list's other viewable files to swipe through.
  function openViewer(e) {
    const withPath = (x) => ({ ...x, path: pathOf(x) });
    const list = shown.some((x) => pathOf(x) === pathOf(e)) ? shown : [e];
    F.view(withPath(e), list.map(withPath));
  }

  // Thumbnails (photos, and a frame of each video): lazily, as they come into view; cached.
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
  }, { rootMargin: "300px" }) : null;
  function thumb(el, path, mtime) {
    if (!io) return;
    el._thumb = { path, mtime };
    io.observe(el);
  }

  // ── selection bar, picker bar ────────────────────────────────────────────────
  const selected = () => [...S.sel];
  const entryOf = (p) => shown.find((e) => pathOf(e) === p) || S.entries.find((e) => join(S.path, e.name) === p) || { name: p.split("/").pop(), dir: false, path: p };
  function selbar() {
    const n = S.sel.size;
    const one = n === 1 ? entryOf(selected()[0]) : null;
    const trashy = inTrash(S.path);
    return h("div", { class: "fselbar" },
      btn("x", "Clear selection", () => { S.sel.clear(); S.selMode = false; render(); }),
      h("b", {}, `${n} selected`),
      h("span", { class: "fgrow" }),
      btn("selectall", "Select all", () => { for (const e of shown) S.sel.add(pathOf(e)); S.selMode = true; render(); }),
      trashy && rw() ? btn("undo", "Restore", () => restore(selected())) : null,
      btn("download", "Download", () => { for (const p of selected()) { const e = entryOf(p); F.download(p, e.name, e.dir); } S.sel.clear(); S.selMode = false; render(); F.showTransfers(true); }),
      one && !one.dir && canOpenThere() ? btn("monitor", "Open on the computer", () => openThere(selected()[0])) : null,
      !trashy && rw() ? btn("copy", "Copy to…", () => pick("copy", selected())) : null,
      !trashy && rw() ? btn("move", "Move to…", () => pick("move", selected())) : null,
      !trashy && rw() && one ? btn("edit", "Rename", () => rename(selected()[0])) : null,
      !trashy && rw() ? btn("trash", "Move to Trash", () => trash(selected())) : null,
      one ? btn("dots", "More", () => itemSheet(one)) : null,
    );
  }
  function pick(op, paths) {
    S.picker = { op, paths, from: S.path };
    S.sel.clear(); S.selMode = false;
    if (S.view !== "dir") go(S.path && rootOf(S.path) ? S.path : F.info.home); else render();
  }
  function pickbar() {
    const n = S.picker.paths.length;
    const verb = S.picker.op === "move" ? "Move" : "Copy";
    return h("div", { class: "fpickbar" },
      h("span", {}, `${verb} ${n} item${n > 1 ? "s" : ""} into `, h("b", {}, label(S.path || ""))),
      h("span", { class: "fgrow" }),
      h("button", { class: "fgo ghost", onclick: () => { S.picker = null; render(); } }, "Cancel"),
      rw() ? h("button", { class: "fgo ghost", disabled: S.view !== "dir", onclick: newFolder }, "New folder") : null,
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
      { label: "Move to Trash — it can be restored", run: () => { S.sel.clear(); S.selMode = false; act(F.req("trash", { paths }), n === 1 ? "Moved to the Trash" : `${n} items moved to the Trash`); } },
      { label: "Cancel", primary: true },
    ]);
  }
  async function restore(paths) {
    S.sel.clear(); S.selMode = false;
    let ok = 0;
    for (const path of paths) {
      try { const r = await F.req("restore", { path }); ok++; if (paths.length === 1) F.toast("Restored to " + tidy(parentOf(r.to))); }
      catch (e) { F.toast(`${path.split("/").pop()}: ${e.message || e}`, true); }
    }
    if (paths.length > 1 && ok) F.toast(`${ok} items restored`);
    F.refresh();
  }
  async function newFolder() {
    const name = await F.prompt("New folder", "", "Create");
    if (name) act(F.req("mkdir", { path: join(S.path, name) }));
  }
  function pin(path, on) { act(F.req(on ? "pin" : "unpin", { path }), on ? "Pinned" : "Unpinned").then(() => loadQuick().then(render)); }
  const pinned = (p) => S.quick.pins.some((x) => x.path === p && x.kind === "pin");

  // Everything that can be done to one item.
  function itemSheet(e) {
    const p = pathOf(e);
    const opts = [];
    const trashy = inTrash(p);
    if (e.dir) opts.push({ label: "Open", run: () => go(p) });
    else if (F.viewable(e.name)) opts.push({ label: "Open here", run: () => openViewer(e) });
    if (!e.dir && canOpenThere() && !trashy) opts.push({ label: "Open on the computer", run: () => openThere(p) });
    if (trashy && rw()) opts.push({ label: "Restore", run: () => restore([p]) });
    opts.push({ label: e.dir ? "Download as .zip" : "Download to this device", run: () => { F.download(p, e.name, e.dir); F.showTransfers(true); } });
    if (canShare(e)) opts.push({ label: "Share…", run: () => share(e) });
    if (e.recent) opts.push({ label: "Show in folder", run: () => go(parentOf(p)) });
    if (rw() && !trashy) {
      opts.push({ label: "Rename", run: () => rename(p) });
      opts.push({ label: "Copy to…", run: () => pick("copy", [p]) });
      opts.push({ label: "Move to…", run: () => pick("move", [p]) });
      if (e.dir) opts.push({ label: pinned(p) ? "Unpin" : "Pin to quick access", run: () => pin(p, !pinned(p)) });
    }
    opts.push({ label: "Copy path", run: () => copyPath(p) });
    opts.push({ label: "Properties", run: () => properties(e) });
    if (rw() && !trashy) opts.push({ label: "Move to Trash", run: () => trash([p]) });
    opts.push({ label: "Cancel", primary: true });
    W.sheet.ask("files-item", `${e.name}${e.dir ? "" : " · " + F.fmtSize(e.size)}`, opts);
  }
  function properties(e) {
    const p = pathOf(e);
    const k = F.kind(e.name, e.dir);
    const kindName = e.dir ? "Folder" : { image: "Image", video: "Video", audio: "Audio", archive: "Archive", pdf: "PDF document", doc: "Document", sheet: "Spreadsheet", slides: "Presentation", code: "Source code", app: "Program" }[k.icon] || "File";
    const row = (a, b) => h("div", { class: "fprop" }, h("small", {}, a), h("span", {}, b));
    const box = h("div", { class: "fmodal", onclick: (ev) => { if (ev.target === box) box.remove(); } },
      h("div", { class: "fdialog" },
        h("div", { class: "fprophd" }, h("span", { class: "fdisc big", style: `--hue:${k.hue}` }, ico(k.icon)), h("b", {}, e.name)),
        row("Type", kindName + (e.link ? " (a link)" : "")),
        e.dir ? null : row("Size", `${F.fmtSize(e.size)} (${(e.size || 0).toLocaleString()} bytes)`),
        row("Modified", e.mtime ? new Date(e.mtime * 1000).toLocaleString() : "—"),
        row("Where", tidy(parentOf(p))),
        h("div", { class: "fdbtns" },
          h("button", { class: "fgo ghost", onclick: () => copyPath(p) }, "Copy path"),
          h("button", { class: "fgo", onclick: () => box.remove() }, "Done"))));
    document.body.appendChild(box);
  }

  // Hidden pickers for uploads; `capture` opens the camera on a phone.
  function choose(kind) {
    const inp = h("input", { type: "file", multiple: kind !== "camera", style: "display:none" });
    if (kind === "folder") inp.webkitdirectory = true;
    if (kind === "camera") { inp.accept = "image/*,video/*"; inp.setAttribute("capture", "environment"); }
    inp.addEventListener("change", () => { if (inp.files.length) { F.upload(S.path, [...inp.files]); F.showTransfers(true); } inp.remove(); });
    document.body.appendChild(inp);
    inp.click();
  }
  function addSheet() {
    const opts = [
      { label: "Upload files", run: () => choose("files") },
      { label: "Upload a folder", run: () => choose("folder") },
    ];
    if (touchy()) opts.push({ label: "Take a photo or video", run: () => choose("camera") });
    opts.push({ label: "New folder", run: newFolder }, { label: "Cancel", primary: true });
    W.sheet.ask("files-add", "Add to " + label(S.path), opts);
  }
  function moreSheet() {
    const opts = [];
    const dir = S.view === "dir" && !inTrash(S.path);
    if (dir && rw()) {
      opts.push({ label: "Upload files", run: () => choose("files") }, { label: "Upload a folder", run: () => choose("folder") }, { label: "New folder", run: newFolder });
      opts.push({ label: pinned(S.path) ? "Unpin this folder" : "Pin this folder", run: () => pin(S.path, !pinned(S.path)) });
    }
    if (dir) {
      opts.push({ label: "Download this folder (.zip)", run: () => { F.download(S.path, label(S.path), true); F.showTransfers(true); } });
      if (canOpenThere()) opts.push({ label: "Open this folder on the computer", run: () => openThere(S.path) });
      opts.push({ label: "Copy path", run: () => copyPath(S.path) });
    }
    opts.push({ label: S.hidden ? "Hide hidden files" : "Show hidden files", run: () => { S.hidden = !S.hidden; keepView(); render(); } });
    opts.push({ label: "Refresh", run: F.refresh });
    if (rw()) opts.push({ label: "Device access…", run: () => F.devicesPanel() });
    if (!wide()) opts.push({ label: "Close files", run: F.closeUi });
    opts.push({ label: "Cancel", primary: true });
    W.sheet.ask("files-more", S.view === "dir" ? label(S.path) : "Files", opts);
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
    if (F.inField(e)) return;
    if (e.key === "Escape") { e.preventDefault(); if (S.picker) { S.picker = null; render(); } else back(); return; }
    if ((e.key === "/" || ((e.ctrlKey || e.metaKey) && e.key === "f")) && F.ready) {
      e.preventDefault(); S.searching = true; render(); setTimeout(() => root.querySelector(".fsearch input")?.focus(), 30); return;
    }
    if (S.picker) return;
    if (e.key === "Backspace" || (e.altKey && e.key === "ArrowUp")) { e.preventDefault(); back(); }
    else if ((e.ctrlKey || e.metaKey) && e.key === "a") { e.preventDefault(); for (const x of shown) S.sel.add(pathOf(x)); render(); }
    else if (e.key === "Delete" && S.sel.size && rw() && !inTrash(S.path)) trash(selected());
    else if (e.key === "F2" && S.sel.size === 1 && rw()) rename(selected()[0]);
    else if (e.key === "Enter" && S.sel.size === 1) activate(entryOf(selected()[0]));
    else if ((e.ctrlKey || e.metaKey) && e.key === "l" && S.view === "dir") { e.preventDefault(); copyPath(S.path); }
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
    if (m.op === "play") return; // watching is logged, not announced
    const what = { upload: "uploaded", trash: "moved to the Trash", restore: "restored", rename: "renamed", move: "moved", copy: "copied", mkdir: "created", download: "downloaded", "download-zip": "downloaded", play: "is playing", pin: "pinned", unpin: "unpinned", grant: "changed file access:" }[m.op] || m.op;
    emit({ type: "notification", id: 0x7f000000 + Math.floor(Math.random() * 0xffffff), app: "Files", summary: `${m.device} ${what}`, body: m.path });
    if (F.isOpen() && S.view === "dir" && !["download", "download-zip", "play"].includes(m.op)) F.refresh();
  });

  W.filesOpen = () => F.openUi();
  W.filesClose = () => F.closeUi();
})();
