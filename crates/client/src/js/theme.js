// wado bridge — base16 theming. Owns the 16 CSS custom properties (--base00…--base0F) that
// every stylesheet draws from, so a scheme change is 16 property writes and no re-render.
//
// Lives in JS rather than Rust for one reason: the saved scheme is applied during the
// bridge's synchronous head, before Rust has hydrated any signal, so the page never flashes
// the default palette first. Rust owns the *picker* (which name is selected, what was pasted)
// and hands 16 values down through W.setTheme; parsing pasted schemes is Rust's job.

// Bundled schemes, base00…base0F in order. All dark: the chrome surrounds a video surface,
// and a light ground bleeds onto the picture (see the stage backdrop override in stage.css).
W.SCHEMES = {
  "default-dark": ["181818","282828","383838","585858","b8b8b8","d8d8d8","e8e8e8","f8f8f8",
                   "ab4642","dc9656","f7ca88","a1b56c","86c1b9","7cafc2","ba8baf","a16946"],
  "gruvbox-dark": ["1d2021","3c3836","504945","665c54","bdae93","d5c4a1","ebdbb2","fbf1c7",
                   "fb4934","fe8019","fabd2f","b8bb26","8ec07c","83a598","d3869b","d65d0e"],
  "nord":         ["2e3440","3b4252","434c5e","4c566a","d8dee9","e5e9f0","eceff4","8fbcbb",
                   "bf616a","d08770","ebcb8b","a3be8c","88c0d0","81a1c1","b48ead","5e81ac"],
  "tomorrow-night":["1d1f21","282a2e","373b41","969896","b4b7b4","c5c8c6","e0e0e0","ffffff",
                   "cc6666","de935f","f0c674","b5bd68","8abeb7","81a2be","b294bb","a3685a"],
};

W.DEFAULT_SCHEME = "default-dark";

// `hexes` is 16 bare hex triples (no '#'), base00 first. Anything else is ignored rather
// than half-applied — a partially themed UI is worse than an unchanged one.
W.applyTheme = (hexes) => {
  if (!Array.isArray(hexes) || hexes.length !== 16) return false;
  const root = document.documentElement.style;
  hexes.forEach((h, i) => root.setProperty("--base0" + i.toString(16).toUpperCase(), "#" + h));
  return true;
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
})();
