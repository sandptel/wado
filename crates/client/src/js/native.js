// wado bridge — the Android app's native side (android/), when this page runs inside it.
//
// Feature-detected, never assumed: `window.wadoNative` exists only in the app's WebView, where
// WebViewCompat.addWebMessageListener injects it for this page's own origin alone
// (https://appassets.androidplatform.net). In a browser `W.native` is null and the native_*.js
// files that follow do nothing at all — the web build behaves exactly as before.
//
// Messages are JSON objects with a `t` (type), both ways:
//   page → app   W.native.send(t, fields)
//   app → page   W.native.on(t, fn)    one handler per type; fn(msg)
//   W.native.info   {platform: "android", version, sdk} — set by the app before this page ran
// The protocol is listed in docs/android.md, and android/…/bridge/BridgeProtocol.kt is its mirror.

(() => {
  W.native = null;
  const N = window.wadoNative;
  if (!N || typeof N.postMessage !== "function") return;
  const handlers = {};
  const send = (t, fields) => {
    try { N.postMessage(JSON.stringify(Object.assign({ t }, fields || {}))); } catch (_) {}
  };
  N.onmessage = (e) => {
    let m;
    try { m = JSON.parse(e.data); } catch (_) { return; }
    const fn = m && handlers[m.t];
    if (fn) { try { fn(m); } catch (err) { console.warn("wado native:", m.t, err); } }
  };
  W.native = {
    info: window.wadoNativeInfo || { platform: "android" },
    send,
    on: (t, fn) => { handlers[t] = fn; },
  };
  // Back from the app's navigation (the system back gesture), when the page has no history of
  // its own left: Rust steps back a view, or says there is nowhere left (`nativeExit`).
  W.native.on("back", () => emit({ type: "nativeBack" }));
  // Open a computer (and a door on it) that the app's own home picked.
  W.native.on("go", (m) => emit({ type: "nativeGo", key: m.key || "", go: m.go || "" }));
  W.nativeExit = () => send("exit");
})();
