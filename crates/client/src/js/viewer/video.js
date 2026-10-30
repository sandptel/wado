// wado bridge — the viewer's video: streamed, never fetched whole. The computer turns the file
// into fragmented MP4 (copied when the browser can decode it, converted when not) and the player
// (js/files_player.js over Plyr) plays it as it arrives — any format ffmpeg reads.
//
//   V.playIn(ctx)   shared with js/viewer/audio.js: the player in the stage, the probe's facts
//                   in the details, the next file of the same kind when this one ends

(() => {
  const F = W.files;
  const V = F.viewer;
  const h = F.h;

  const clock = (s) => {
    s = Math.round(s || 0);
    const t = [Math.floor(s / 3600), Math.floor(s / 60) % 60, s % 60];
    return (t[0] ? t[0] + ":" + String(t[1]).padStart(2, "0") : t[1]) + ":" + String(t[2]).padStart(2, "0");
  };

  V.playIn = (ctx) => {
    const { e, stage } = ctx;
    stage.replaceChildren(h("div", { class: "fspin", role: "status", "aria-label": "Loading" }));
    if (ctx.kind === "video") stage.classList.add("dark");
    let p = null, gone = false;
    F.player(e, stage, { onEnded: ctx.ended, onError: (why) => ctx.fail(why) }).then((pl) => {
      if (gone || !ctx.alive()) { pl.destroy(); return; }
      p = pl;
      const m = p.media;
      if (m) m.addEventListener("loadedmetadata", () => {
        const s = p.state || {};
        ctx.add(ctx.kind === "video" ? "Video" : "Sound", [
          ["Length", isFinite(m.duration) && m.duration > 0 ? clock(m.duration) : ""],
          ["Picture", m.videoWidth ? `${m.videoWidth} × ${m.videoHeight}` : ""],
          ["Played", s.kind || ""],
        ]);
      }, { once: true });
    });
    // Shown until it is ready; one that becomes ready after the viewer moved on is ended.
    return {
      get media() { return p && p.media; },
      destroy() { gone = true; if (p) p.destroy(); },
    };
  };

  V.add("video", { fetch: "none", render: V.playIn });
})();
