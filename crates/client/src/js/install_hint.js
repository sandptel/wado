// wado bridge — iOS has no install prompt, so Safari there is told once how to put wado on the
// Home Screen (ui/landing/install_hint.rs). Once means once: the hint is marked seen the moment it
// is decided on, so it shows on this load and never again, and its × only hides it sooner.
// Not in other iOS browsers (they cannot add to the Home Screen the same way), not when already
// installed.
(() => {
  const KEY = "wado.iosHint";
  const ua = navigator.userAgent || "";
  const ios = /iP(hone|od|ad)/.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1);
  const safari = /Safari\//.test(ua) && !/(CriOS|FxiOS|EdgiOS|OPiOS|GSA|DuckDuckGo)\//.test(ua);
  let installed = navigator.standalone === true;
  try { installed = installed || matchMedia("(display-mode: standalone)").matches; } catch (_) {}
  let seen = true;
  try { seen = !!localStorage.getItem(KEY); } catch (_) {}
  W.iosHint = ios && safari && !installed && !seen;
  if (W.iosHint) { try { localStorage.setItem(KEY, String(Date.now())); } catch (_) {} }
})();
