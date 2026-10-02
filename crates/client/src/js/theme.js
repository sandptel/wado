// wado bridge — base16 theming. Owns the 16 CSS custom properties (--base00…--base0F) that
// every stylesheet draws from, so a scheme change is 16 property writes and no re-render.
//
// Lives in JS rather than Rust for one reason: the saved scheme is applied during the
// bridge's synchronous head, before Rust has hydrated any signal, so the page never flashes
// the default palette first. Rust owns the *picker* (which name is selected, what was pasted)
// and hands 16 values down through W.setTheme; parsing pasted schemes is Rust's job.

// Bundled schemes, base00…base0F in order. All dark: the chrome surrounds a video surface,
// and a light ground bleeds onto the picture (see the stage backdrop override in stage.css).
// `W.SCHEME_LIST` is spliced in ahead of this file from `src/schemes.json` (see bridge.rs) —
// the one list both this file and the Rust picker read, so they cannot drift.
W.SCHEMES = Object.fromEntries(W.SCHEME_LIST);

W.DEFAULT_SCHEME = "default-dark";

// `hexes` is 16 bare hex triples (no '#'), base00 first. Anything else is ignored rather
// than half-applied — a partially themed UI is worse than an unchanged one.
W.applyTheme = (hexes) => {
  if (!Array.isArray(hexes) || hexes.length !== 16) return false;
  const root = document.documentElement;
  hexes.forEach((h, i) => root.style.setProperty("--base0" + i.toString(16).toUpperCase(), "#" + h));
  // Light or dark ground, by base00's luminance — what native controls and the scrollbar
  // follow. The stage behind the video stays black either way (stage.css).
  const n = parseInt(hexes[0], 16);
  const lum = 0.2126 * (n >> 16) + 0.7152 * ((n >> 8) & 255) + 0.0722 * (n & 255);
  root.dataset.ground = lum > 140 ? "light" : "dark";
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.content = "#" + hexes[0];
  return true;
};

// The rest of the look: corner style, motion level and which slot is the accent. Attributes
// and one variable, so the stylesheets own what each value means.
W.setLook = (radius, motion, accent) => {
  const root = document.documentElement;
  root.dataset.radius = radius || "round";
  root.dataset.motion = motion || "full";
  root.style.setProperty("--accent", "var(--base" + (accent || "0D") + ")");
};

// Called by the Rust picker. `custom` (16 parsed values) wins over `name` when present, which
// is what makes a pasted scheme survive a reload without being added to W.SCHEMES.
W.setTheme = (name, custom) => {
  if (W.applyTheme(custom)) return;
  W.applyTheme(W.SCHEMES[name] || W.SCHEMES[W.DEFAULT_SCHEME]);
};

// Apply the saved scheme now, before Rust renders anything.
(() => {
  const s = W.loadSettings();
  W.setTheme(s.theme, s.theme_custom);
  W.setLook(s.radius, s.motion, s.accent);
})();
