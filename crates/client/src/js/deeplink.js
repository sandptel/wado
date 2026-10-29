// wado bridge — the shortcut that opened this page: `?go=trackpad|files|shell|session` from the
// app icon's long-press menu (public/manifest.webmanifest). Taken out of the address at once, so
// a reload or a bookmark does not repeat it; Rust takes it once (`W.takeGo`) and follows it on
// the last used computer (deeplink.rs). Runs before storage.js, whose connect link rewrites the
// address.
(() => {
  let go = "";
  try {
    const q = new URLSearchParams(location.search);
    if (q.has("go")) {
      go = q.get("go") || "";
      q.delete("go");
      const rest = q.toString();
      history.replaceState(null, "", location.pathname + (rest ? "?" + rest : "") + location.hash);
    }
  } catch (_) {}
  W.takeGo = () => { const g = go; go = ""; return g; };
})();
