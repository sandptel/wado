// wado bridge — the phone's font size, in the Android app.
//
// The app pins the WebView's textZoom at 100: left to itself the WebView multiplies every font by
// the phone's font scale on top of a layout drawn for 100%, and text overflows its boxes. The scale
// is handed over instead — in `wadoNativeInfo.fontScale` before the page runs (read by
// `ui_size.js`), and as `{t:"fontScale", scale}` when it changes — so Appearance's "Follow phone"
// applies it through the design's own type scale. Does nothing in a browser.

(() => {
  if (!W.native || !W.uiSize) return;
  W.native.on("fontScale", (m) => W.uiSize.device(m.scale));
})();
