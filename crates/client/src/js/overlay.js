// wado bridge — debug "show touches" overlay. A transparent, pointer-events:none canvas
// pinned over the viewport; when enabled, a short-lived dot is drawn at each touch/click
// point so input landing spots are visible while debugging. Purely client-side.

W.overlay = {
  el: null,
  ensure() {
    if (W.overlay.el) return W.overlay.el;
    const c = document.createElement("canvas");
    c.id = "wado-touch-overlay";
    c.style.cssText = "position:fixed;left:0;top:0;pointer-events:none;z-index:9999;";
    document.body.appendChild(c);
    W.overlay.el = c;
    return c;
  },
  mark(clientX, clientY) {
    if (!W.showTouches) return;
    const c = W.overlay.ensure();
    if (c.width !== window.innerWidth) c.width = window.innerWidth;
    if (c.height !== window.innerHeight) c.height = window.innerHeight;
    const ctx = c.getContext("2d");
    ctx.beginPath();
    ctx.arc(clientX, clientY, 18, 0, Math.PI * 2);
    ctx.fillStyle = "rgba(46,200,120,0.45)";
    ctx.fill();
    setTimeout(() => { try { ctx.clearRect(clientX - 20, clientY - 20, 40, 40); } catch (_) {} }, 220);
  },
  clear() {
    const c = W.overlay.el;
    if (c) { const ctx = c.getContext("2d"); if (ctx) ctx.clearRect(0, 0, c.width, c.height); }
  },
};

W.setShowTouches = (on) => {
  W.showTouches = !!on;
  if (!on) W.overlay.clear();
};
