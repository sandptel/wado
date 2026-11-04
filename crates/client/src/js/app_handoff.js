// wado bridge — a pairing link opened in a browser on Android (Google Lens, the camera app, a
// message) hands itself to the wado app when it is installed: Chrome's `intent:` link names the
// app's package and its own `wado://` scheme, which needs no domain verification. Not installed: Chrome follows the fallback
// back here with `app=0`, and this page pairs as it always has. Runs before anything dials.
(() => {
  try {
    const u = new URL(location.href);
    if (!/Android/i.test(navigator.userAgent) || !u.searchParams.get("pair") || u.searchParams.get("app") === "0") return;
    // Inside the app's own WebView, or a standalone install of this page, there is nothing to hand to.
    if (window.wadoNativeInfo || matchMedia("(display-mode: standalone)").matches) return;
    const back = new URL(location.href);
    back.searchParams.set("app", "0");
    // The app's own scheme, not https: Android 12+ will not hand an unverified web link to an app
    // even by package name, but a custom scheme needs no verification (wado://pair?…).
    W.appHandoff = `intent://pair${u.search}#Intent;scheme=wado;package=io.github.sandptel.wado.next;S.browser_fallback_url=${encodeURIComponent(back.href)};end`;
    location.replace(W.appHandoff);
  } catch (_) {}
})();
