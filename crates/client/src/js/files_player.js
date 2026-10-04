// wado bridge — the media player of the file viewer: Plyr's controls (pinned by SRI in
// index.html) over a streamed <video>/<audio> (js/files_stream.js), plus what VLC has and Plyr
// does not: audio-track choice, subtitles (embedded, files beside the video, or a file from this
// device) with delay and size, aspect ratio, zoom, rotation, and VLC's keys.
//
//   const p = await W.files.player(entry, stage, { onEnded, onError })   p.destroy()
//
// Subtitles are drawn by this file, not by the browser's track renderer: cues are parsed here
// from WebVTT (the daemon converts every format to it), so a delay is just arithmetic and the
// text sits inside Plyr's container, which is what goes fullscreen. Cue text is set with
// textContent — subtitle files are data, never markup.
//
// Keys (VLC's): space play · ←/→ 10 s · ↑/↓ volume · f fullscreen · m mute · [ ] speed ·
// = normal speed · v subtitles · b audio track · a aspect · z zoom · g/h subtitle delay ·
// r rotate · i info.

(() => {
  const F = W.files;
  const h = F.h;
  const ico = (n) => h("span", { class: "fi", html: F.icon(n) });
  const SPEEDS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2, 3];
  const ASPECTS = [["Fit", "contain", ""], ["Fill (crop)", "cover", ""], ["Stretch", "fill", ""], ["16:9", "fill", "16/9"], ["4:3", "fill", "4/3"], ["21:9", "fill", "21/9"], ["1:1", "fill", "1/1"]];
  const ZOOMS = [1, 1.25, 1.5, 2];
  const POS = "wado.files.pos";

  // Plyr draws icons from an SVG sprite; ours is vendored (crates/client/src/js/vendor), injected
  // once into the document, so Plyr never fetches one from a CDN at run time.
  let sprite = false;
  function spriteOnce() {
    if (sprite || !W.PLYR_SPRITE) return;
    sprite = true;
    const d = document.createElement("div");
    d.hidden = true;
    d.innerHTML = W.PLYR_SPRITE;
    document.body.appendChild(d);
  }

  // WebVTT → [{ s, e, t }]. Tolerates SRT's commas, cue settings, styling tags (stripped).
  F.parseVtt = (text) => {
    const ts = (x) => { const p = x.trim().replace(",", ".").split(":").map(Number); return p.length === 3 ? p[0] * 3600 + p[1] * 60 + p[2] : p[0] * 60 + p[1]; };
    const cues = [];
    for (const block of text.replace(/\r/g, "").split(/\n\n+/)) {
      const lines = block.split("\n");
      const i = lines.findIndex((l) => l.includes("-->"));
      if (i < 0) continue;
      const [a, b] = lines[i].split("-->");
      const t = lines.slice(i + 1).join("\n").replace(/<[^>]*>/g, "").replace(/\{\\[^}]*\}/g, "").trim();
      const s = ts(a), e = ts(b.trim().split(/\s+/)[0]);
      if (t && isFinite(s) && isFinite(e)) cues.push({ s, e, t });
    }
    return cues.sort((x, y) => x.s - y.s);
  };

  const langName = (l) => { try { return l ? new Intl.DisplayNames([navigator.language], { type: "language" }).of(l) || l : ""; } catch (_) { return l; } };
  const trackLabel = (x, i) => [x.title, langName(x.lang)].filter(Boolean).join(" · ") || `Track ${i + 1}`;

  F.player = async (entry, stage, opts = {}) => {
    spriteOnce();
    const audioOnly = F.viewable(entry.name) === "audio";
    const media = h(audioOnly ? "audio" : "video", { playsinline: true, preload: "auto", class: "fpvideo" });
    const subsBox = h("div", { class: "fpsubs", "aria-live": "off" });
    const box = h("div", { class: "fplayer" + (audioOnly ? " audio" : "") },
      audioOnly ? h("div", { class: "fpcover" }, ico("audio"), h("b", {}, entry.name)) : null, media);
    stage.replaceChildren(box);

    const st = { cues: [], delay: 0, size: 1, subLabel: "Off", aspect: 0, zoom: 0, rot: 0, audio: 0, info: null, kind: "" };
    let stream = null, plyr = null, panel = null, raf = 0, dead = false;
    const fail = (why) => { if (!dead && opts.onError) opts.onError(why); };

    try {
      stream = await F.mediaOpen(entry.path, media, {
        start: resumeAt(),
        onError: fail,
        onInfo: (i) => { st.kind = i.transcode ? "converted on the computer" : "played directly"; },
      });
    } catch (e) {
      fail(String(e.message || e));
      return { destroy() {} };
    }
    if (dead) { stream.destroy(); return { destroy() {} }; }
    st.info = stream.info;

    // On a touch screen the controls are laid out for a thumb (files.css, `.fplayer.touch`): the
    // seek bar gets a full-width row above the buttons, and taps are ours (see `gestures`).
    const touch = matchMedia("(pointer: coarse)").matches;
    if (touch) box.classList.add("touch");
    if (window.Plyr) {
      try {
        plyr = new window.Plyr(media, {
          // A phone gets one row of buttons: ±10 s is a double-tap there, picture-in-picture and
          // the volume slider are the phone's own; everything else stays.
          controls: touch
            ? ["play-large", "play", "progress", "current-time", "duration", "mute", "settings", "fullscreen"]
            : ["play-large", "rewind", "play", "fast-forward", "progress", "current-time", "duration", "mute", "volume", "settings", "pip", "fullscreen"],
          displayDuration: true,
          clickToPlay: !touch,
          hideControls: true,
          settings: ["speed", "loop"],
          speed: { selected: 1, options: SPEEDS },
          seekTime: 10,
          keyboard: { focused: false, global: false },
          tooltips: { controls: true, seek: true },
          loadSprite: false,
          iconUrl: "",
          fullscreen: { enabled: true, fallback: true, iosNative: false },
          invertTime: false,
          resetOnEnd: false,
          storage: { enabled: false },
        });
      } catch (_) { plyr = null; }
    }
    if (!plyr) media.controls = true; // the browser's own controls if Plyr did not load
    const container = () => (plyr && plyr.elements && plyr.elements.container) || box;
    container().append(subsBox);
    // The VLC button, beside Plyr's settings cog.
    const vlc = h("button", { class: "plyr__control fpvlcbtn", type: "button", "aria-label": "Tracks, subtitles and picture", title: "Tracks, subtitles and picture (VLC menu)", onclick: (e) => { e.stopPropagation(); togglePanel(); } }, ico("sliders"));
    const controls = plyr && plyr.elements && plyr.elements.controls;
    if (controls) controls.insertBefore(vlc, controls.querySelector("[data-plyr='settings']")?.parentElement || null);
    else box.append(vlc);

    media.play().catch(() => {});
    media.addEventListener("ended", () => { forgetPos(); if (opts.onEnded) opts.onEnded(); });
    media.addEventListener("error", () => fail("the browser could not play this stream"));

    // ── position memory: a film reopened carries on where it was left ──
    function resumeAt() {
      try { const p = JSON.parse(localStorage.getItem(POS) || "{}")[entry.path]; return p && p.m === entry.mtime ? p.t : 0; } catch (_) { return 0; }
    }
    function savePos() {
      try {
        const all = JSON.parse(localStorage.getItem(POS) || "{}");
        const t = media.currentTime, d = media.duration || 0;
        if (t > 30 && (!d || t < d - 30)) all[entry.path] = { t, m: entry.mtime, at: Date.now() };
        else delete all[entry.path];
        const keep = Object.entries(all).sort((a, b) => b[1].at - a[1].at).slice(0, 200);
        localStorage.setItem(POS, JSON.stringify(Object.fromEntries(keep)));
      } catch (_) {}
    }
    function forgetPos() { try { const all = JSON.parse(localStorage.getItem(POS) || "{}"); delete all[entry.path]; localStorage.setItem(POS, JSON.stringify(all)); } catch (_) {} }
    const posTimer = setInterval(savePos, 5000);

    // ── subtitles ──
    function drawSubs() {
      raf = requestAnimationFrame(drawSubs);
      const t = media.currentTime - st.delay;
      const now = st.cues.filter((c) => t >= c.s && t <= c.e).map((c) => c.t).join("\n");
      if (subsBox.textContent !== now) subsBox.textContent = now;
    }
    raf = requestAnimationFrame(drawSubs);
    async function useSubs(label, getVtt) {
      st.subLabel = label;
      if (!getVtt) { st.cues = []; renderPanel(); return; }
      try { st.cues = F.parseVtt(await getVtt()); F.toast(`Subtitles: ${label}${st.cues.length ? "" : " (empty)"}`); }
      catch (e) { st.cues = []; st.subLabel = "Off"; F.toast(String(e.message || e), true); }
      renderPanel();
    }
    const subOptions = () => {
      const out = [["Off", null]];
      (st.info.subs || []).forEach((s, i) => { if (s.text) out.push([trackLabel(s, i) + " (in the file)", () => F.req("subs", { path: entry.path, track: i }).then((r) => r.vtt)]); });
      (st.info.sidecars || []).forEach((s) => out.push([s.name, () => F.req("subs", { path: entry.path, sidecar: s.path }).then((r) => r.vtt)]));
      return out;
    };
    function loadLocalSubs() {
      const inp = h("input", { type: "file", accept: ".srt,.vtt,.txt", style: "display:none" });
      inp.addEventListener("change", () => { const f = inp.files[0]; if (f) useSubs(f.name, () => f.text()); inp.remove(); });
      document.body.appendChild(inp);
      inp.click();
    }
    // The first sidecar, or the first text track, turns on by itself — as VLC does.
    { const first = subOptions()[1]; if (first) useSubs(first[0], first[1]); }

    // ── picture ──
    function applyPicture() {
      const [, fit, ratio] = ASPECTS[st.aspect];
      media.style.objectFit = fit;
      media.style.aspectRatio = ratio || "";
      media.style.height = ratio ? "auto" : "";
      media.style.transform = `rotate(${st.rot}deg) scale(${ZOOMS[st.zoom]})`;
      subsBox.style.fontSize = `calc(${st.size} * clamp(15px, 3.4vmin, 34px))`;
    }

    // ── the VLC menu ──
    const cycle = (k, n) => { st[k] = (st[k] + 1) % n; applyPicture(); renderPanel(); };
    function setAudio(i) { st.audio = i; stream.setAudio(i); renderPanel(); F.toast("Audio: " + trackLabel(st.info.audio[i], i)); }
    function togglePanel() { if (panel) { panel.remove(); panel = null; } else { panel = h("div", { class: "fpanel-vlc", onclick: (e) => e.stopPropagation() }); container().append(panel); renderPanel(); } }
    function renderPanel() {
      if (!panel) return;
      const row = (label, kids) => h("div", { class: "fvrow" }, h("small", {}, label), h("div", { class: "fvopts" }, kids));
      const opt = (label, on, run) => h("button", { class: on ? "on" : "", onclick: run }, label);
      const a = st.info.audio || [];
      const v = st.info.video;
      panel.replaceChildren(
        h("div", { class: "fvhd" }, h("b", {}, "Playback"), h("button", { class: "fbtn", "aria-label": "Close", onclick: togglePanel }, ico("x"))),
        a.length > 1 ? row("Audio track", a.map((x, i) => opt(trackLabel(x, i) + (x.channels ? ` · ${x.channels}ch` : ""), st.audio === i, () => setAudio(i)))) : null,
        row("Subtitles", [...subOptions().map(([l, g]) => opt(l, st.subLabel === l, () => useSubs(l, g))), opt("From this device…", false, loadLocalSubs)]),
        st.cues.length ? row(`Subtitle delay ${st.delay > 0 ? "+" : ""}${st.delay.toFixed(1)} s`, [
          opt("−0.5", false, () => { st.delay -= 0.5; renderPanel(); }), opt("−0.1", false, () => { st.delay -= 0.1; renderPanel(); }),
          opt("0", st.delay === 0, () => { st.delay = 0; renderPanel(); }),
          opt("+0.1", false, () => { st.delay += 0.1; renderPanel(); }), opt("+0.5", false, () => { st.delay += 0.5; renderPanel(); })]) : null,
        st.cues.length ? row("Subtitle size", [0.75, 1, 1.3, 1.6].map((z) => opt({ 0.75: "S", 1: "M", 1.3: "L", 1.6: "XL" }[z], st.size === z, () => { st.size = z; applyPicture(); renderPanel(); }))) : null,
        !audioOnly ? row("Aspect", ASPECTS.map(([l], i) => opt(l, st.aspect === i, () => { st.aspect = i; applyPicture(); renderPanel(); }))) : null,
        !audioOnly ? row("Zoom", ZOOMS.map((z, i) => opt(z + "×", st.zoom === i, () => { st.zoom = i; applyPicture(); renderPanel(); }))) : null,
        !audioOnly ? row("Rotate", [0, 90, 180, 270].map((r) => opt(r + "°", st.rot === r, () => { st.rot = r; applyPicture(); renderPanel(); }))) : null,
        h("p", { class: "fvinfo" }, [v ? `${v.codec} ${v.width}×${v.height}` : "", a[st.audio] ? a[st.audio].codec : "", st.kind].filter(Boolean).join(" · ")),
      );
    }

    // ── keys ──
    function keys(e) {
      if (dead || F.inField(e) || e.ctrlKey || e.metaKey || e.altKey) return;
      const k = e.key;
      const did = () => { e.preventDefault(); e.stopPropagation(); };
      const rate = (d) => { const i = SPEEDS.indexOf(media.playbackRate); const n = SPEEDS[Math.max(0, Math.min(SPEEDS.length - 1, (i < 0 ? 3 : i) + d))]; media.playbackRate = n; F.toast(`Speed ${n}×`); };
      if (k === " " || k === "k") { did(); media.paused ? media.play().catch(() => {}) : media.pause(); }
      else if (k === "ArrowRight" && !e.shiftKey) { did(); media.currentTime = Math.min((media.duration || 1e9) - 0.5, media.currentTime + 10); }
      else if (k === "ArrowLeft" && !e.shiftKey) { did(); media.currentTime = Math.max(0, media.currentTime - 10); }
      else if (k === "ArrowUp") { did(); media.volume = Math.min(1, media.volume + 0.05); }
      else if (k === "ArrowDown") { did(); media.volume = Math.max(0, media.volume - 0.05); }
      else if (k === "f") { did(); if (plyr) plyr.fullscreen.toggle(); else (document.fullscreenElement ? document.exitFullscreen() : box.requestFullscreen()).catch(() => {}); }
      else if (k === "m") { did(); media.muted = !media.muted; }
      else if (k === "]") { did(); rate(1); }
      else if (k === "[") { did(); rate(-1); }
      else if (k === "=") { did(); media.playbackRate = 1; F.toast("Speed 1×"); }
      else if (k === "v") { did(); const o = subOptions(); const i = o.findIndex(([l]) => l === st.subLabel); const n = o[(i + 1) % o.length]; useSubs(n[0], n[1]); }
      else if (k === "b" && (st.info.audio || []).length > 1) { did(); setAudio((st.audio + 1) % st.info.audio.length); }
      else if (k === "a" && !audioOnly) { did(); cycle("aspect", ASPECTS.length); F.toast("Aspect: " + ASPECTS[st.aspect][0]); }
      else if (k === "z" && !audioOnly) { did(); cycle("zoom", ZOOMS.length); F.toast(`Zoom ${ZOOMS[st.zoom]}×`); }
      else if (k === "r" && !audioOnly) { did(); st.rot = (st.rot + 90) % 360; applyPicture(); renderPanel(); }
      else if (k === "g" || k === "h") { did(); st.delay += k === "h" ? 0.1 : -0.1; F.toast(`Subtitle delay ${st.delay.toFixed(1)} s`); renderPanel(); }
      else if (k === "i") { did(); togglePanel(); }
    }
    addEventListener("keydown", keys, true);
    const ungesture = touch && !audioOnly ? gestures() : () => {};

    // ── touch: the gestures a phone video player is expected to have ──
    //   tap               show / hide the controls
    //   double-tap ← / →  10 s back / forward (taps in a row add up: 20 s, 30 s…)
    //   double-tap middle play / pause
    //   hold              2× speed while held
    //   pinch out / in    fill the screen (crop) / fit
    //   turn to landscape fullscreen
    function gestures() {
      const wrap = (plyr && plyr.elements && plyr.elements.wrapper) || box;
      const ripple = h("div", { class: "fpripple" });
      container().append(ripple);
      let tapAt = 0, tapX = 0, single = null, hold = null, held = false, streak = 0, pinch0 = 0;
      const flash = (side, text) => {
        ripple.className = "fpripple " + side;
        ripple.textContent = text;
        void ripple.offsetWidth;
        ripple.classList.add("show");
      };
      const seek = (d) => {
        media.currentTime = Math.max(0, Math.min((media.duration || 1e9) - 0.3, media.currentTime + d));
      };
      const down = (e) => {
        if (e.touches && e.touches.length === 2) {
          clearTimeout(hold);
          pinch0 = Math.hypot(e.touches[0].clientX - e.touches[1].clientX, e.touches[0].clientY - e.touches[1].clientY);
          return;
        }
        held = false;
        clearTimeout(hold);
        hold = setTimeout(() => { held = true; media.dataset.rate = media.playbackRate; media.playbackRate = 2; flash("mid", "2× ▸▸"); }, 400);
      };
      const move = (e) => {
        if (!(e.touches && e.touches.length === 2 && pinch0)) return;
        const d = Math.hypot(e.touches[0].clientX - e.touches[1].clientX, e.touches[0].clientY - e.touches[1].clientY);
        if (Math.abs(d - pinch0) < 60) return;
        st.aspect = d > pinch0 ? 1 : 0;
        applyPicture(); renderPanel();
        flash("mid", d > pinch0 ? "Fill" : "Fit");
        pinch0 = 0;
      };
      const up = (e) => {
        clearTimeout(hold);
        if (held) { held = false; media.playbackRate = Number(media.dataset.rate) || 1; ripple.classList.remove("show"); e.preventDefault(); return; }
        if (e.touches && e.touches.length) return;
        const t = e.changedTouches && e.changedTouches[0];
        if (!t || e.target.closest(".plyr__controls, .fpanel-vlc, .plyr__control--overlaid")) return;
        const r = wrap.getBoundingClientRect();
        const x = (t.clientX - r.left) / r.width;
        const now = Date.now();
        if (now - tapAt < 300 && Math.abs(t.clientX - tapX) < 80) {
          clearTimeout(single);
          single = null;
          if (x < 0.38) { streak = Math.min(streak + 1, 9); seek(-10); flash("left", `◂◂ ${streak * 10} s`); }
          else if (x > 0.62) { streak = Math.min(streak + 1, 9); seek(10); flash("right", `${streak * 10} s ▸▸`); }
          else { media.paused ? media.play().catch(() => {}) : media.pause(); flash("mid", media.paused ? "❚❚" : "▶"); }
          tapAt = now; // a third tap continues the streak
        } else {
          streak = 0;
          tapAt = now; tapX = t.clientX;
          single = setTimeout(() => { single = null; if (plyr) plyr.toggleControls(); }, 300);
        }
        e.preventDefault();
      };
      wrap.addEventListener("touchstart", down, { passive: true });
      wrap.addEventListener("touchmove", move, { passive: true });
      wrap.addEventListener("touchend", up);
      // A held finger is our 2× — not the browser's "save video" menu, which would also cancel it.
      const noMenu = (e) => e.preventDefault();
      wrap.addEventListener("contextmenu", noMenu);
      wrap.addEventListener("touchcancel", () => { clearTimeout(hold); if (held) { held = false; media.playbackRate = Number(media.dataset.rate) || 1; } });
      // Landscape → fullscreen, and back, while this is playing.
      // The browser may refuse real fullscreen without a tap, so landscape also makes the viewer
      // itself edge to edge (`.fview.land`: no title bar, the picture fills the screen).
      const view = box.closest(".fview");
      const orient = screen.orientation;
      const landQ = matchMedia("(orientation: landscape)");
      const turn = () => {
        // The viewport's own shape: `screen.orientation` describes the device, which a desktop
        // browser emulating a phone (and some foldables) report as landscape when it is not.
        const land = landQ.matches;
        if (view) view.classList.toggle("land", land);
        if (!plyr) return;
        // Real fullscreen is asked for, and a refusal (no tap behind it) is fine — Plyr follows
        // `fullscreenchange`, so its button stays in step either way.
        const c = container();
        if (land && !media.paused && !document.fullscreenElement && c.requestFullscreen) c.requestFullscreen({ navigationUI: "hide" }).catch(() => {});
        else if (!land && document.fullscreenElement) document.exitFullscreen().catch(() => {});
      };
      turn();
      landQ.addEventListener("change", turn);
      // Fullscreen on a phone: turn the screen to the video's own shape, where the browser lets us.
      const fs = () => {
        if (!plyr || !plyr.fullscreen.active || !orient || !orient.lock) return;
        try { orient.lock(media.videoWidth >= media.videoHeight ? "landscape" : "portrait").catch(() => {}); } catch (_) {}
      };
      if (plyr) plyr.on("enterfullscreen", fs);
      return () => {
        clearTimeout(hold); clearTimeout(single);
        if (view) view.classList.remove("land");
        landQ.removeEventListener("change", turn);
        try { if (orient && orient.unlock) orient.unlock(); } catch (_) {}
      };
    }
    applyPicture();

    return {
      media,
      get state() { return st; },
      destroy() {
        if (dead) return;
        dead = true;
        savePos();
        clearInterval(posTimer);
        cancelAnimationFrame(raf);
        removeEventListener("keydown", keys, true);
        ungesture();
        if (stream) stream.destroy();
        try { if (plyr) plyr.destroy(); } catch (_) {}
      },
    };
  };
})();
