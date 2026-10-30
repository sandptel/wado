// wado bridge — the viewer's sound: a simple player with the title, no waveform. Streamed like
// video (js/viewer/video.js), so a long recording or an album side starts at once; the next song
// in the folder plays when this one ends.

(() => {
  const V = W.files.viewer;
  V.add("audio", { fetch: "none", render: (ctx) => V.playIn(ctx) });
})();
