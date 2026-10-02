// wado bridge — the lens: a circular magnifier over the stream for targets too small for a
// finger (M-P S4/S6).
//
// Tap-to-lens, as decided (Decision Log 2026-09-29): an ambiguous tap does not click; it opens
// this lens, centred on the spot, and it *stays*. A tap inside lands exactly where it points
// in the magnified picture; a tap outside closes it and clicks nothing; left alone it closes
// itself. Two taps and no dexterity, and nothing is ever clicked by accident.
//
// Drawn from the video element itself every frame, so the picture inside is live. Opened by
// the tap decision in input_tap.js, or on demand with a two-finger tap.
//
// Also here: the pixel heuristic used when an app exposes no accessibility tree — whether the
// picture around a point is busy enough that a finger is probably ambiguous there.

const LENS_CSS = 190;   // diameter on screen
const LENS_ZOOM = 3;
const LENS_IDLE_MS = 5000;

// The "Lens" tile: off, an unsure tap clicks instead of magnifying. The two-finger tap still
// opens the lens on purpose.
W.lensAuto = true;
W.setLensAuto = (on) => { W.lensAuto = !!on; };

W.lens = {
  el: null,
  raf: null,
  timer: null,
  centre: null, // client point the lens magnifies

  open(clientX, clientY) {
    const video = W.videoEl;
    if (!video) return;
    W.lens.close();
    const c = document.createElement("canvas");
    const dpr = window.devicePixelRatio || 1;
    c.width = c.height = Math.round(LENS_CSS * dpr);
    c.className = "wado-lens";
    // Centred on the point, kept on screen.
    const half = LENS_CSS / 2;
    const left = Math.min(Math.max(clientX - half, 4), window.innerWidth - LENS_CSS - 4);
    const top = Math.min(Math.max(clientY - half, 4), window.innerHeight - LENS_CSS - 4);
    c.style.left = left + "px";
    c.style.top = top + "px";
    document.body.appendChild(c);
    W.lens.el = c;
    W.lens.centre = { x: clientX, y: clientY };
    const ctx = c.getContext("2d");
    const draw = () => {
      const k = W.targets.content(video);
      if (k) {
        // Source square in video pixels around the centre.
        const scale = video.videoWidth / k.w;
        const srcCss = LENS_CSS / LENS_ZOOM;
        const sx = (W.lens.centre.x - k.left - srcCss / 2) * scale;
        const sy = (W.lens.centre.y - k.top - srcCss / 2) * scale;
        ctx.fillStyle = "#000";
        ctx.fillRect(0, 0, c.width, c.height);
        ctx.drawImage(video, sx, sy, srcCss * scale, srcCss * scale, 0, 0, c.width, c.height);
      }
      W.lens.raf = requestAnimationFrame(draw);
    };
    draw();
    W.lens.timer = setTimeout(W.lens.close, LENS_IDLE_MS);
  },

  close() {
    if (W.lens.raf != null) cancelAnimationFrame(W.lens.raf);
    clearTimeout(W.lens.timer);
    if (W.lens.el) W.lens.el.remove();
    W.lens.el = null;
    W.lens.raf = null;
    W.lens.centre = null;
  },

  // The client point in the real picture that a point inside the lens shows.
  unmagnify(clientX, clientY) {
    const r = W.lens.el.getBoundingClientRect();
    return {
      x: W.lens.centre.x + (clientX - (r.left + r.width / 2)) / LENS_ZOOM,
      y: W.lens.centre.y + (clientY - (r.top + r.height / 2)) / LENS_ZOOM,
    };
  },

  // With no accessibility tree to ask: is the picture around this point busy — many sharp
  // edges, as dense controls are? A guess by design (text is busy too); used only when the
  // app says nothing about itself.
  dense(clientX, clientY) {
    const video = W.videoEl;
    const k = video && W.targets.content(video);
    if (!k) return false;
    const N = 32, box = 56; // sample a 56-css-px square at 32×32
    const scale = video.videoWidth / k.w;
    const cv = W.lens._probe || (W.lens._probe = document.createElement("canvas"));
    cv.width = cv.height = N;
    const g = cv.getContext("2d", { willReadFrequently: true });
    try {
      g.drawImage(video, (clientX - k.left - box / 2) * scale, (clientY - k.top - box / 2) * scale,
        box * scale, box * scale, 0, 0, N, N);
      return W.lens.edgeFraction(g.getImageData(0, 0, N, N).data, N) > 0.16;
    } catch (_) {
      return false; // a tainted or not-yet-decoded frame: say "not dense" and click plainly
    }
  },

  // Fraction of pixels whose luma differs sharply from the right or lower neighbour (pure —
  // see scripts/lens-check.mjs).
  edgeFraction(rgba, n) {
    const L = (i) => 0.299 * rgba[i] + 0.587 * rgba[i + 1] + 0.114 * rgba[i + 2];
    let edges = 0;
    for (let y = 0; y < n - 1; y++) {
      for (let x = 0; x < n - 1; x++) {
        const i = (y * n + x) * 4;
        if (Math.abs(L(i) - L(i + 4)) > 40 || Math.abs(L(i) - L(i + n * 4)) > 40) edges++;
      }
    }
    return edges / ((n - 1) * (n - 1));
  },
};

// While the lens is open it owns the next touch, wherever it lands: inside, a precise click;
// outside, a dismissal. Capture phase on the document, so the video never sees that touch.
document.addEventListener("pointerdown", (e) => {
  const lens = W.lens.el;
  if (!lens) return;
  e.preventDefault();
  e.stopPropagation();
  const r = lens.getBoundingClientRect();
  const dx = e.clientX - (r.left + r.width / 2), dy = e.clientY - (r.top + r.height / 2);
  if (Math.hypot(dx, dy) <= r.width / 2) {
    const p = W.lens.unmagnify(e.clientX, e.clientY);
    const n = W.normPoint(p.x, p.y, W.videoEl);
    if (n) {
      W.coalesce.now({ t: "button", x: n.x, y: n.y, button: "left", pressed: true });
      W.sendInput({ t: "button", x: n.x, y: n.y, button: "left", pressed: false });
    }
  }
  W.lens.close();
}, true);
