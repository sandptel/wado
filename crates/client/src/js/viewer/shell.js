// wado bridge — the viewer: one layer over the file manager that shows a file without
// downloading it. This file is the frame; each kind of file is drawn by its own module
// (js/viewer/<kind>.js, picked by js/viewer/kinds.js).
//
//   ┌ back · name (truncated in the middle, so the extension shows) · size · type · ⋯ ┐
//   ├ the file, filling the rest                                         ‹ ›  (wide) ┤
//   └ its tools (zoom, wrap…)                                            Details ˄    ┘
//
// The details are a sheet pulled up from the footer (a panel beside the file on a wide screen);
// every action — download, share, copy path, open the folder, details — is in ⋯. Previous and
// next: a swipe, the arrows on a wide screen, ← → (n / p while something plays).
//
// **Security, load-bearing:** the bytes come from the computer, and a blob: URL has *this page's*
// origin — the origin that holds the device key. So nothing that can run script is ever handed
// to the browser as a document: HTML is shown as source text, SVG only ever as an <img> (where
// scripts never run), Markdown is built as DOM text. Never "fix" this with an iframe or innerHTML.
//
//   W.files.view(entry, siblings)    W.files.viewClose()

(() => {
  const F = W.files;
  const V = F.viewer;
  const h = F.h;
  const ico = V.ico;

  let el = null, list = [], at = 0, abort = {}, live = null, details = null, opener = null, dir = 0;
  // The module's player, zoom or listener for the file on screen: torn down before the next one.
  const endLive = () => { if (live) { try { live.destroy(); } catch (_) {} live = null; } };

  const close = () => {
    if (!el) return;
    if (abort.stop) abort.stop();
    endLive();
    V.menuClose(false);
    const old = el;
    el = null;
    removeEventListener("keydown", keys, true);
    removeEventListener("resize", relayout);
    const files = document.getElementById("wado-files");
    if (files) files.inert = false;
    // It leaves on a short fade, already out of reach, then is gone from the page.
    old.inert = true;
    old.classList.add("out");
    const gone = () => old.remove();
    old.addEventListener("animationend", gone, { once: true });
    setTimeout(gone, 240);
    if (opener && opener.isConnected) try { opener.focus({ preventScroll: true }); } catch (_) {}
  };
  F.viewClose = close;
  const relayout = () => details && details.relayout();

  F.view = (entry, siblings) => {
    // The gallery is the folder's files, in the order shown.
    list = (siblings || [entry]).filter((x) => !x.dir);
    at = Math.max(0, list.findIndex((x) => x.path === entry.path));
    if (!list.length) list = [entry];
    if (!el) {
      opener = document.activeElement;
      el = h("div", { class: "fview", role: "dialog", "aria-modal": "true", "aria-label": "Viewer" });
      document.body.appendChild(el);
      // The file manager under it is covered: never tapped or focused through the viewer.
      const files = document.getElementById("wado-files");
      if (files) files.inert = true;
      addEventListener("keydown", keys, true);
      addEventListener("resize", relayout);
      swipe(el);
    }
    dir = 0;
    show();
  };

  function keys(e) {
    if (!el || F.inField(e)) return;
    if (e.key === "Escape") {
      e.stopPropagation(); e.preventDefault();
      if (V.menuOpen()) V.menuClose();
      else if (details && details.isOpen()) details.close();
      else close();
      return;
    }
    if (V.menuOpen()) return;
    if (e.key === "i" && !e.ctrlKey && !e.metaKey) { e.preventDefault(); details.toggle(); }
    // While a video or song plays, ←/→ seek (the player's keys); n / p move through the folder.
    else if (e.key === "n" || (e.key === "ArrowRight" && !(live && live.media))) { e.preventDefault(); step(1); }
    else if (e.key === "p" || (e.key === "ArrowLeft" && !(live && live.media))) { e.preventDefault(); step(-1); }
  }
  function step(d) {
    const n = at + d;
    if (n < 0 || n >= list.length) return;
    at = n;
    dir = d;
    show();
  }
  // Swipe left/right between files (not while zoomed in, not on a player's controls, a table or
  // a text that scrolls sideways).
  function swipe(node) {
    let x0 = null, y0 = 0;
    const skip = (t) => t.closest && t.closest(".plyr__controls, input, .fpanel-vlc, .fvdetails, .fvfoot, .fvhead, .fvscrollx");
    node.addEventListener("touchstart", (e) => {
      x0 = null;
      if (e.touches.length !== 1 || node.classList.contains("zoomed") || skip(e.target)) return;
      if (e.target.closest && e.target.closest(".fplayer.touch") && !(live && live.media && live.media.paused)) return;
      x0 = e.touches[0].clientX; y0 = e.touches[0].clientY;
    }, { passive: true });
    node.addEventListener("touchend", (e) => {
      if (x0 === null) return;
      const dx = e.changedTouches[0].clientX - x0, dy = e.changedTouches[0].clientY - y0;
      x0 = null;
      if (Math.abs(dx) > 60 && Math.abs(dx) > Math.abs(dy) * 1.5) step(dx < 0 ? 1 : -1);
    });
  }

  // The name, truncated in the middle: the head gives way, the last few letters and the
  // extension always show.
  function nameParts(name) {
    const dot = name.lastIndexOf(".");
    const cut = Math.max(0, (dot > 0 ? dot : name.length) - 6);
    return h("b", { class: "fvname2", title: name },
      h("span", { class: "fvnh" }, name.slice(0, cut)), h("span", { class: "fvnt" }, name.slice(cut)));
  }

  function menuItems(e) {
    const A = F.act || {};
    return [
      { label: "Details", icon: "info", run: () => details.open() },
      { label: "Download", icon: "download", run: () => { F.download(e.path, e.name, false); F.toast("Downloading " + e.name); } },
      A.canShare && A.canShare(e) ? { label: "Share", icon: "share", run: () => A.share(e) } : null,
      A.copyPath ? { label: "Copy path", icon: "copy", run: () => A.copyPath(e.path) } : null,
      A.showIn ? { label: "Open folder", icon: "folder", run: () => A.showIn(e.path) } : null,
      A.canOpenThere && A.canOpenThere() ? { label: "Open on the computer", icon: "monitor", run: () => A.openThere(e.path) } : null,
    ].filter(Boolean);
  }

  function show() {
    if (abort.stop) abort.stop();
    endLive();
    V.menuClose(false);
    abort = {};
    const e = list[at];
    const kind = V.kind(e.name);
    const def = V.get(kind);
    const bar = h("div", { class: "fvbar", role: "progressbar", "aria-label": "Loading", "aria-valuemin": "0", "aria-valuemax": "100" }, h("i"));
    const stage = h("div", { class: "fvstage", "data-kind": kind });
    const where = list.length > 1 ? `${at + 1} of ${list.length}` : "";
    const more = h("button", { class: "fvbtn fvmore", "aria-label": "More actions", "aria-haspopup": "menu", "aria-expanded": "false",
      onclick: () => V.menu(more, menuItems(e)) }, ico("dots"));
    const head = h("header", { class: "fvhead" },
      h("button", { class: "fvbtn", "aria-label": "Close viewer", onclick: close }, ico("back")),
      h("div", { class: "fvtitle" }, nameParts(e.name),
        h("small", {}, [F.fmtSize(e.size), V.type(e.name), where].filter(Boolean).join(" · "))),
      more);
    const tools = h("div", { class: "fvtoolslot" });
    const dbtn = h("button", { class: "fvdbtn", "aria-label": "Details", onclick: () => details.toggle() }, ico("info"), h("span", {}, "Details"));
    const foot = h("footer", { class: "fvfoot" }, tools, dbtn);
    pullUp(foot);
    const nav = [
      at > 0 ? h("button", { class: "fvnav prev", "aria-label": "Previous", onclick: () => step(-1) }, ico("back")) : null,
      at < list.length - 1 ? h("button", { class: "fvnav next", "aria-label": "Next", onclick: () => step(1) }, ico("next")) : null,
    ].filter(Boolean);
    const wasOpen = details && details.isOpen();
    el.classList.remove("zoomed", "bare", "detailed");
    el.setAttribute("aria-label", e.name);
    el.dataset.kind = kind;
    el.replaceChildren(head, bar, stage, foot, ...nav);
    details = V.details(el);
    details.fill(e);
    if (wasOpen && matchMedia("(min-width: 900px)").matches) details.open();
    if (dir) stage.style.setProperty("--from", dir > 0 ? "24px" : "-24px");
    stage.classList.add("fvswap");
    if (!el.contains(document.activeElement)) head.querySelector(".fvbtn").focus({ preventScroll: true });

    const ctx = {
      e, kind, stage, list, at,
      tools: (...kids) => tools.replaceChildren(...kids.filter(Boolean)),
      add: (title, rows) => details.add(title, rows),
      bare: () => el && el.classList.toggle("bare"),
      zoomed: (on) => el && el.classList.toggle("zoomed", on),
      step,
      fail: (why) => { if (el && list[at] === e) { endLive(); stage.replaceChildren(V.cantPreview(e, why)); } },
      alive: () => !!el && list[at] === e,
      ended: () => { const n = list[at + 1]; if (n && V.kind(n.name) === kind) step(1); },
    };
    const draw = (v) => {
      ctx.bytes = v;
      bar.classList.add("done");
      try { live = def.render(ctx) || null; }
      catch (err) { ctx.fail("This file could not be shown here: " + (err.message || err)); }
      if (live && live.media) el.dataset.media = "1"; else delete el.dataset.media;
      prefetch();
    };
    const load = () => {
      if (def.fetch === "none") return draw(null);
      stage.replaceChildren(h("div", { class: "fspin", role: "status", "aria-label": "Loading" }));
      const mine = abort;
      V.bytes(e, {
        type: V.mime(e.name),
        limit: def.fetch === "head" ? def.limit || V.TEXT_MAX : 0,
        abort,
        progress: (got, size) => {
          if (abort !== mine) return;
          const pct = size ? Math.min(100, (got / size) * 100) : 30;
          bar.firstChild.style.scale = pct / 100 + " 1";
          bar.setAttribute("aria-valuenow", String(Math.round(pct)));
        },
      }).then((v) => { if (abort === mine && el) draw(v); }, (err) => {
        if (abort !== mine || !el || err.message === "stopped") return;
        bar.classList.add("done");
        stage.replaceChildren(V.cantPreview(e, "It did not come through: " + String(err.message || err)));
      });
    };
    // The relay's limited lane carries listings and small files only; a bigger preview waits.
    const small = (e.size || 0) <= V.LANE_MAX;
    const waits = V.lane() === "fallback" && !small && !V.cached(e) && (def.fetch !== "none" || kind === "video" || kind === "audio");
    if (waits) {
      bar.classList.add("done");
      stage.replaceChildren(V.waitDirect(e));
    } else if (def.fetch === "whole" && e.size > V.ASK_OVER && !V.cached(e)) {
      bar.classList.add("done");
      stage.replaceChildren(V.askLoad(e, load));
    } else load();
  }

  // On a phone the footer is the sheet's handle: pull it up for the details.
  function pullUp(foot) {
    let y0 = null;
    foot.addEventListener("pointerdown", (ev) => { if (ev.pointerType !== "mouse" && !ev.target.closest(".fvtoolslot button")) y0 = ev.clientY; });
    foot.addEventListener("pointermove", (ev) => { if (y0 !== null && y0 - ev.clientY > 36) { y0 = null; details.open(); } });
    const end = () => { y0 = null; };
    foot.addEventListener("pointerup", end);
    foot.addEventListener("pointercancel", end);
  }

  // The next picture, ahead of time, so a swipe through photos does not wait each time.
  function prefetch() {
    const n = list[at + 1];
    if (n && V.kind(n.name) === "image" && n.size < (16 << 20) && !(V.lane() === "fallback" && n.size > V.LANE_MAX)) {
      V.bytes(n, { type: V.mime(n.name) }).catch(() => {});
    }
  }
})();
