// wado bridge — the file viewer: open a file from the file manager without downloading it.
//
// Everything here is the browser's own renderer — <img>, <video>, <audio>, the PDF viewer, a
// <pre> — fed the file's bytes over the files channel (checked against the host's SHA-256 like
// any download). Video and audio stream instead (js/files_player.js, js/files_stream.js); photos
// zoom with Panzoom. Both libraries are SRI-pinned in index.html and only ever handle bytes.
//
// **Security, load-bearing:** the bytes come from the computer, and a blob: URL has *this page's*
// origin — the origin that holds the device key. So nothing that can run script is ever handed
// to the browser as a document: HTML and SVG-as-document are shown as source text; SVG is only
// ever an <img> (where scripts never run). Never "fix" this by rendering HTML in an iframe.
//
//   W.files.viewable(name) → kind | null      W.files.view(entry, siblings)

(() => {
  const F = W.files;
  const h = F.h;
  const ico = (n) => h("span", { class: "fi", html: F.icon(n) });

  const KINDS = {
    image: ["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "ico", "svg"],
    // Any format ffmpeg reads: the computer converts what the browser cannot play.
    video: ["mp4", "m4v", "webm", "mov", "mkv", "ogv", "avi", "wmv", "flv", "mpg", "mpeg", "ts", "m2ts", "mts", "3gp", "vob", "divx", "rmvb", "asf"],
    audio: ["mp3", "m4a", "aac", "ogg", "oga", "opus", "flac", "wav", "weba", "wma", "ape", "alac", "aiff", "aif", "mka", "ac3", "dts", "amr"],
    pdf: ["pdf"],
    text: ["txt", "md", "markdown", "log", "json", "csv", "tsv", "xml", "yaml", "yml", "toml", "ini", "conf", "cfg", "kdl", "nix",
      "rs", "js", "mjs", "ts", "tsx", "jsx", "py", "go", "c", "h", "cc", "cpp", "hpp", "java", "kt", "rb", "php", "sh", "fish",
      "zsh", "bash", "lua", "sql", "swift", "zig", "hs", "ml", "css", "scss", "html", "htm", "srt", "vtt", "diff", "patch", "env"],
  };
  const MIME = {
    png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp", avif: "image/avif",
    bmp: "image/bmp", ico: "image/x-icon", svg: "image/svg+xml", mp4: "video/mp4", m4v: "video/mp4", webm: "video/webm",
    mov: "video/mp4", mkv: "video/webm", ogv: "video/ogg", mp3: "audio/mpeg", m4a: "audio/mp4", aac: "audio/aac",
    ogg: "audio/ogg", oga: "audio/ogg", opus: "audio/ogg", flac: "audio/flac", wav: "audio/wav", weba: "audio/webm",
    pdf: "application/pdf",
  };
  // What is fetched before asking: past this, "this is big — load it?"
  const ASK_OVER = 300 << 20;
  // Text is shown up to this much; the rest is cut, and said so.
  const TEXT_MAX = 2 << 20;

  const ext = (name) => { const i = name.lastIndexOf("."); return i > 0 ? name.slice(i + 1).toLowerCase() : ""; };
  F.viewable = (name) => {
    const e = ext(name);
    for (const [k, list] of Object.entries(KINDS)) if (list.includes(e)) return k;
    return null;
  };

  // ── bytes → Blob, with progress; cached for the gallery ──────────────────────
  const cache = new Map(); // path@mtime → { url, blob, cut }
  function remember(key, v) {
    cache.set(key, v);
    while (cache.size > 8) { const [k, old] = cache.entries().next().value; URL.revokeObjectURL(old.url); cache.delete(k); }
  }
  F.viewerForget = () => { for (const v of cache.values()) URL.revokeObjectURL(v.url); cache.clear(); };

  // Resolves `{ url, blob, cut }`. `limit`: stop after this many bytes (text).
  function fetchFile(e, type, limit, progress, abort) {
    const key = e.path + "@" + e.mtime;
    if (cache.has(key)) return Promise.resolve(cache.get(key));
    return F.connect().then(() => new Promise((ok, no) => {
      const parts = [];
      const sha = W.sha256();
      let got = 0, size = e.size || 0, cut = false;
      const id = F.stream("get", { path: e.path, offset: 0 }, {
        bytes(u8) {
          if (cut) return;
          parts.push(u8); sha.update(u8); got += u8.length;
          progress(got, size);
          if (limit && got >= limit) {
            cut = true;
            F.forget(id);
            F.req("cancel", { xfer: id }).catch(() => {});
            finish();
          }
        },
        msg(m) {
          if (m.err) { no(new Error(m.err)); return true; }
          if (m.done) {
            if (sha.hex() !== m.sha256) { no(new Error("the file arrived damaged — try again")); return true; }
            finish();
            return true;
          }
          if (m.size !== undefined) size = m.size;
          return false;
        },
      });
      abort.stop = () => { F.forget(id); F.req("cancel", { xfer: id }).catch(() => {}); no(new Error("stopped")); };
      function finish() {
        const blob = new Blob(parts, { type });
        const v = { url: URL.createObjectURL(blob), blob, cut };
        remember(key, v);
        ok(v);
      }
    }));
  }

  // A file's bytes as a Blob (hash-checked), for sharing it to another app on this device.
  F.fetchFile = (e) => fetchFile(e, MIME[ext(e.name)] || "application/octet-stream", 0, () => {}, {});

  // ── the viewer ───────────────────────────────────────────────────────────────
  let el = null, list = [], at = 0, abort = {}, live = null;
  // The player or zoom of the file on screen: torn down before the next one, and on close.
  const endLive = () => { if (live) { try { live.destroy(); } catch (_) {} live = null; } };
  const close = () => {
    if (abort.stop) abort.stop();
    endLive();
    if (el) el.remove();
    el = null;
    removeEventListener("keydown", keys, true);
  };
  F.viewClose = close;

  F.view = (entry, siblings) => {
    // The gallery is the viewable files of the folder, in the order shown.
    list = (siblings || [entry]).filter((x) => !x.dir && F.viewable(x.name));
    at = Math.max(0, list.findIndex((x) => x.path === entry.path));
    if (!list.length) list = [entry];
    if (!el) {
      el = h("div", { class: "fview", role: "dialog", "aria-label": "Viewer" });
      document.body.appendChild(el);
      addEventListener("keydown", keys, true);
      swipe(el);
    }
    show();
  };

  function keys(e) {
    if (!el) return;
    if (e.key === "Escape") { e.stopPropagation(); e.preventDefault(); close(); }
    // While a video or song plays, ←/→ seek (the player's keys); n / p move through the folder.
    else if (e.key === "n" || (e.key === "ArrowRight" && !(live && live.media))) { e.preventDefault(); step(1); }
    else if (e.key === "p" || (e.key === "ArrowLeft" && !(live && live.media))) { e.preventDefault(); step(-1); }
  }
  function step(d) {
    const n = at + d;
    if (n < 0 || n >= list.length) return;
    at = n;
    show();
  }
  // Swipe left/right between files (not while zoomed into an image).
  function swipe(node) {
    let x0 = null, y0 = 0;
    node.addEventListener("touchstart", (e) => { if (e.touches.length === 1 && !node.classList.contains("zoomed") && !(e.target.closest && e.target.closest(".plyr__controls, input, .fpanel-vlc"))) { x0 = e.touches[0].clientX; y0 = e.touches[0].clientY; } }, { passive: true });
    node.addEventListener("touchend", (e) => {
      if (x0 === null) return;
      const dx = e.changedTouches[0].clientX - x0, dy = e.changedTouches[0].clientY - y0;
      x0 = null;
      if (Math.abs(dx) > 60 && Math.abs(dx) > Math.abs(dy) * 1.5) step(dx < 0 ? 1 : -1);
    });
  }

  function show() {
    if (abort.stop) abort.stop();
    endLive();
    abort = {};
    const e = list[at];
    const kind = F.viewable(e.name);
    const bar = h("div", { class: "fvbar" }, h("i"));
    const stage = h("div", { class: "fvstage" });
    const head = h("header", { class: "fvhead" },
      h("button", { class: "fbtn", "aria-label": "Close viewer", onclick: close }, ico("back")),
      h("div", { class: "fvtitle" }, h("b", {}, e.name), h("small", {}, [F.fmtSize(e.size), list.length > 1 ? `${at + 1} of ${list.length}` : ""].filter(Boolean).join(" · "))),
      h("button", { class: "fbtn", "aria-label": "Download", title: "Download", onclick: () => { F.download(e.path, e.name, false); F.toast("Downloading " + e.name); } }, ico("download")),
      h("button", { class: "fbtn", "aria-label": "Close", onclick: close }, ico("x")));
    const nav = list.length > 1 ? [
      at > 0 ? h("button", { class: "fvnav prev", "aria-label": "Previous", onclick: () => step(-1) }, ico("back")) : null,
      at < list.length - 1 ? h("button", { class: "fvnav next", "aria-label": "Next", onclick: () => step(1) }, ico("back")) : null,
    ] : [];
    el.classList.remove("zoomed");
    el.replaceChildren(head, bar, stage, ...nav);

    const load = () => {
      stage.replaceChildren(h("div", { class: "fspin" }));
      const type = kind === "text" ? "text/plain" : MIME[ext(e.name)] || "application/octet-stream";
      const mine = abort;
      fetchFile(e, type, kind === "text" ? TEXT_MAX : 0, (got, size) => {
        if (abort !== mine) return;
        bar.firstChild.style.width = size ? Math.min(100, (got / size) * 100) + "%" : "30%";
      }, abort).then((v) => {
        if (abort !== mine || !el) return;
        bar.classList.add("done");
        render(kind, e, v, stage);
        prefetch();
      }, (err) => {
        if (abort !== mine || !el || err.message === "stopped") return;
        stage.replaceChildren(fallback(e, String(err.message || err)));
      });
    };
    // Video and audio stream: they play as they arrive, whatever their size or format.
    if (kind === "video" || kind === "audio") {
      bar.classList.add("done");
      stage.replaceChildren(h("div", { class: "fspin" }));
      const mine = abort;
      const playing = F.player(e, stage, {
        onEnded: () => { const n = list[at + 1]; if (n && F.viewable(n.name) === kind) step(1); },
        onError: (why) => { if (abort === mine && el) { endLive(); stage.replaceChildren(fallback(e, why)); } },
      });
      // Shown until it is ready; one that becomes ready after the viewer moved on is ended.
      live = { destroy: () => playing.then((p) => p.destroy()) };
      playing.then((p) => { if (abort === mine && el) live = p; else p.destroy(); });
      return;
    }
    if (e.size > ASK_OVER && !cache.has(e.path + "@" + e.mtime)) {
      stage.replaceChildren(h("div", { class: "fvask" }, ico(F.kind(e.name).icon),
        h("p", {}, `${F.fmtSize(e.size)} — it has to come over in full before it plays here.`),
        h("button", { class: "fgo", onclick: load }, "Load it"),
        h("button", { class: "fgo ghost", onclick: () => F.download(e.path, e.name, false) }, "Download instead")));
    } else load();
  }

  // The next image, ahead of time, so a swipe through photos does not wait each time.
  function prefetch() {
    const n = list[at + 1];
    if (n && F.viewable(n.name) === "image" && n.size < (16 << 20)) fetchFile(n, MIME[ext(n.name)], 0, () => {}, {}).catch(() => {});
  }

  // Pinch, wheel, double-tap and drag to zoom and pan a photo (Panzoom, SRI-pinned in
  // index.html); +, - and 0 on a keyboard. While zoomed, a swipe pans instead of changing photo.
  function zoomable(img, stage) {
    if (!window.Panzoom) {
      img.addEventListener("dblclick", () => el.classList.toggle("zoomed"));
      return null;
    }
    const pz = window.Panzoom(img, { maxScale: 12, minScale: 1, step: 0.35, contain: "outside", panOnlyWhenZoomed: true, cursor: "grab" });
    const wheel = (ev) => pz.zoomWithWheel(ev);
    stage.addEventListener("wheel", wheel, { passive: false });
    img.addEventListener("panzoomchange", (ev) => { if (el) el.classList.toggle("zoomed", ev.detail.scale > 1.02); });
    let lastTap = 0;
    const dbl = (ev) => {
      const t = Date.now();
      if (ev.type === "pointerup" && t - lastTap > 300) { lastTap = t; return; }
      lastTap = 0;
      if (pz.getScale() > 1.02) pz.reset();
      else pz.zoomToPoint(3, { clientX: ev.clientX, clientY: ev.clientY });
    };
    img.addEventListener("dblclick", dbl);
    img.addEventListener("pointerup", (ev) => { if (ev.pointerType === "touch") dbl(ev); });
    const keys = (ev) => {
      if (ev.key === "+" || ev.key === "=") { ev.preventDefault(); pz.zoomIn(); }
      else if (ev.key === "-") { ev.preventDefault(); pz.zoomOut(); }
      else if (ev.key === "0") { ev.preventDefault(); pz.reset(); }
    };
    addEventListener("keydown", keys);
    return { destroy() { removeEventListener("keydown", keys); stage.removeEventListener("wheel", wheel); try { pz.destroy(); } catch (_) {} } };
  }

  function fallback(e, why) {
    return h("div", { class: "fvask" }, ico(F.kind(e.name).icon), h("p", {}, why),
      h("button", { class: "fgo", onclick: () => F.download(e.path, e.name, false) }, "Download it"));
  }

  function render(kind, e, v, stage) {
    if (kind === "image") {
      const img = h("img", { class: "fvimg", src: v.url, alt: e.name, draggable: "false" });
      img.onerror = () => stage.replaceChildren(fallback(e, "This browser cannot show this image format."));
      stage.replaceChildren(img);
      live = zoomable(img, stage);
    } else if (kind === "video" || kind === "audio") {
      const m = h(kind, { class: "fv" + kind, src: v.url, controls: true, autoplay: true, playsinline: true });
      m.onerror = () => stage.replaceChildren(fallback(e, `This browser cannot play this ${kind} format (${ext(e.name)}).`));
      stage.replaceChildren(kind === "audio" ? h("div", { class: "fvaudio" }, ico("audio"), h("b", {}, e.name), m) : m);
    } else if (kind === "pdf") {
      // Desktop browsers render PDFs themselves; phone browsers mostly hand them to an app.
      const phone = matchMedia("(pointer: coarse)").matches;
      if (phone) {
        stage.replaceChildren(h("div", { class: "fvask" }, ico("pdf"), h("p", {}, "Phone browsers open PDFs in an app."),
          h("a", { class: "fgo", href: v.url, download: e.name }, "Open / save the PDF")));
      } else stage.replaceChildren(h("iframe", { class: "fvpdf", src: v.url, title: e.name }));
    } else {
      v.blob.text().then((t) => {
        let body = t;
        if (ext(e.name) === "json") { try { body = JSON.stringify(JSON.parse(t), null, 2); } catch (_) {} }
        // textContent, never innerHTML: an .html file is shown as its source.
        const pre = h("pre", { class: "fvtext" });
        pre.textContent = body;
        stage.replaceChildren(v.cut ? h("p", { class: "fwarn" }, `Showing the first ${F.fmtSize(TEXT_MAX)} — download it for the rest.`) : "", pre);
      });
    }
  }
})();
