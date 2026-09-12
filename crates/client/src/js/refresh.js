// wado bridge — what refresh rate is this panel actually running at?
//
// There is no API for it. `requestAnimationFrame` is called once per composite, so the median
// gap between callbacks *is* the refresh interval — and the median, not the mean, because a
// dropped frame doubles one gap and would drag an average up by a rung.
//
// Why it matters here: the fps picker offers 30/60/90/120 with no idea what the panel can show.
// Choosing 120 on a 60 Hz screen is not a free upgrade — it halves the bits available to every
// frame (see the `bits_per_px` line the server logs) to render frames the display never shows.
// Measuring it turns that choice from a guess into a fact.

// ~1 s of frames at 60 Hz. Enough that the median is stable, short enough to run at startup
// without anyone noticing.
const SAMPLES = 60;

// Panels in the wild, so a measurement lands on a name rather than 59.7. Ordered so the
// nearest match wins; anything further than 8 Hz from all of them is reported as measured.
const KNOWN = [24, 30, 48, 50, 60, 72, 75, 90, 100, 120, 144, 165, 240];

W.refreshHz = null;

// Resolves to the measured rate, and caches it — a panel does not change mid-session, and on a
// phone that switches rate dynamically the startup value is the one the user chose fps against.
W.measureRefresh = () =>
  new Promise((resolve) => {
    if (W.refreshHz !== null) return resolve(W.refreshHz);
    if (typeof requestAnimationFrame !== "function") return resolve(null);
    const gaps = [];
    let last = null;
    const step = (t) => {
      if (last !== null) gaps.push(t - last);
      last = t;
      if (gaps.length < SAMPLES) return requestAnimationFrame(step);
      gaps.sort((a, b) => a - b);
      const median = gaps[gaps.length >> 1];
      // A tab in the background is throttled to ~1 fps, which would measure as 1 Hz and stick.
      // Refuse rather than cache a lie; the next call tries again.
      if (!(median > 0.5) || median > 100) return resolve(null);
      const raw = 1000 / median;
      const near = KNOWN.reduce((a, b) => (Math.abs(b - raw) < Math.abs(a - raw) ? b : a));
      W.refreshHz = Math.abs(near - raw) <= 8 ? near : Math.round(raw);
      resolve(W.refreshHz);
    };
    requestAnimationFrame(step);
  });

// Measured once at startup and reported to the UI. Not awaited by anything — the fps picker
// simply gains a line when the number arrives, a second or so in.
W.reportRefresh = async () => {
  const hz = await W.measureRefresh();
  if (hz) emit({ type: "refresh", hz });
};
W.reportRefresh();
