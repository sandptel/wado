// wado bridge — the installed app: registers the service worker that pins this client
// (public/sw.js) and tells Rust what it says.
//   {type:"app", state, version, signed, dev, updated, key, pending, refused, refusedKind,
//    refusedVersion, firstWhy}
// `state` is "pinned" (served from a release the worker holds), "unpinned" (no release here — a
// development server) or "unsupported" (no service worker: plain http, or a browser without one).
// A launch asks the worker to look for a newer release; a page coming back to the front asks
// again (the worker throttles that). An update it refused is also written to the log panel.
(() => {
  const sw = navigator.serviceWorker;
  if (!sw || !window.isSecureContext) { emit({ type: "app", state: "unsupported" }); return; }
  const day = (ms) => {
    try { return new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short" }); } catch (_) { return ""; }
  };
  let told = "";
  sw.addEventListener("message", (e) => {
    const s = e.data;
    if (!s || s.type !== "wado-sw") return;
    const r = s.refused;
    emit({
      type: "app", state: s.pinned ? "pinned" : "unpinned", version: s.version || "", signed: !!s.signed,
      dev: !!s.dev, updated: s.updated ? day(s.updated) : "", key: s.key || "", pending: s.pending || "",
      refused: r ? r.why : "", refusedKind: r ? r.kind : "", refusedVersion: r ? r.version : "",
      firstWhy: s.firstWhy || "",
    });
    if (r && r.id + r.kind !== told) {
      told = r.id + r.kind;
      emit({ type: "log", line: "WARN|" + new Date().toTimeString().slice(0, 8) +
        "|app: refused update " + (r.version || "?") + " (" + r.kind + ") — " + r.why });
    }
  });
  sw.startMessages();
  sw.register("sw.js", { scope: "./", updateViaCache: "none" })
    .then(() => sw.ready)
    .then((reg) => reg.active && reg.active.postMessage({ type: "hello" }))
    .catch((e) => emit({ type: "app", state: "unsupported", refused: String((e && e.message) || e) }));
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState !== "visible") return;
    sw.ready.then((reg) => reg.active && reg.active.postMessage({ type: "check" }));
  });
})();
