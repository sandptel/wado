// Shapes a session can take (port of res/catalog.rs + res/fit.rs): the catalog of common screen
// modes beyond this device's own, and how one fits this screen — fills it, or bars and how much.

const _modes = [
  (1280, 720, '16:9', 'desktop & TV'), (1600, 900, '16:9', 'desktop & TV'), (1920, 1080, '16:9', 'desktop & TV'), (2560, 1440, '16:9', 'desktop & TV'),
  (1280, 800, '16:10', 'laptop'), (1680, 1050, '16:10', 'laptop'), (1920, 1200, '16:10', 'laptop'),
  (1440, 960, '3:2', 'laptop & tablet'), (2256, 1504, '3:2', 'laptop & tablet'),
  (1024, 768, '4:3', 'tablet & older desktop'), (1440, 1080, '4:3', 'tablet & older desktop'), (1600, 1200, '4:3', 'tablet & older desktop'),
  (1280, 1024, '5:4', 'older desktop'),
  (2560, 1080, '21:9', 'ultrawide desktop'), (3440, 1440, '21:9', 'ultrawide desktop'), (3840, 1080, '32:9', 'superwide desktop'),
  (1600, 720, '20:9', 'phone'), (2400, 1080, '20:9', 'phone'), (1560, 720, '19.5:9', 'phone'), (2340, 1080, '19.5:9', 'phone'),
  (1440, 720, '18:9', 'phone & small tablet'), (1080, 1080, '1:1', 'square'),
];

/// The catalog's modes the same way round as this screen, then turned, minus `exclude`.
(List<(String, String)>, List<(String, String)>) grouped(int sw, int sh, List<String> exclude) {
  final portrait = sh > sw;
  return (_oneWay(sw, sh, exclude, portrait), _oneWay(sw, sh, exclude, !portrait));
}

List<(String, String)> _oneWay(int sw, int sh, List<String> exclude, bool portrait) {
  final deviceWay = portrait == (sh > sw);
  final out = <(String, String)>[];
  for (final (long, short, _, cls) in _modes) {
    if (long == short && !deviceWay) continue;
    final (w, h) = portrait ? (short, long) : (long, short);
    final v = '${w}x$h';
    if (exclude.contains(v)) continue;
    out.add((v, label(sw, sh, w, h, cls)));
  }
  return out;
}

enum Fit { fills, pillarbox, letterbox }

(Fit, double) fitOf(int sw, int sh, int w, int h) {
  if (sw == 0 || sh == 0 || w == 0 || h == 0) return (Fit.fills, 1.0);
  final screen = sw / sh, mode = w / h;
  final k = (sw / w) < (sh / h) ? sw / w : sh / h;
  final coverage = (w * k * (h * k)) / (sw * sh);
  if ((mode - screen).abs() / screen <= .005) return (Fit.fills, 1.0);
  return (mode < screen ? Fit.pillarbox : Fit.letterbox, coverage);
}

int _gcd(int a, int b) => b == 0 ? (a < 1 ? 1 : a) : _gcd(b, a % b);

String aspect(int w, int h) {
  if (w == 0 || h == 0) return '—';
  final (long, short) = w >= h ? (w, h) : (h, w);
  final g = _gcd(long, short);
  final (a, b) = (long ~/ g, short ~/ g);
  return b <= 32 ? '$a:$b' : '${(long * 9 / short).toStringAsFixed(1)}:9';
}

String orientation(int w, int h) => w == h ? 'square' : (w > h ? 'landscape' : 'portrait');

String label(int sw, int sh, int w, int h, String cls) {
  final what = '$w × $h — ${aspect(w, h)} ${orientation(w, h)} · $cls';
  final (fit, cov) = fitOf(sw, sh, w, h);
  if (fit == Fit.fills) return '$what · fills this screen';
  final lost = ((1 - cov) * 100).round();
  return '$what · ${fit == Fit.pillarbox ? 'bars at the sides' : 'bars top and bottom'}, $lost% of the screen unused';
}

String verdict(int sw, int sh, int w, int h) {
  final (fit, cov) = fitOf(sw, sh, w, h);
  if (fit == Fit.fills) return 'Fills this screen edge to edge — every part of it is the session, and a tap anywhere lands in it.';
  final lost = ((1 - cov) * 100).round();
  final bars = fit == Fit.pillarbox ? 'down each side' : 'along the top and bottom';
  return "This shape is not your screen's: black bars $bars take $lost% of it. Those bars are not part of the session — a tap there reaches nothing. Pick one marked “fills this screen” to use the whole panel.";
}

/// `"1280x720 — 16:9 landscape · …"` → `"16:9"`: what a chip has room for.
String short(String label) {
  final rest = label.contains(' — ') ? label.split(' — ')[1] : label;
  return rest.split(' · ').first.replaceAll(' landscape', '').replaceAll(' portrait', '');
}

/// What the chosen rate costs on this screen.
String fpsNote(int rung, int hz) {
  if (hz <= 0) return 'This screen’s refresh rate is not known yet.';
  if (rung > hz) return '${(rung - hz) * 100 ~/ rung}% of frames are never shown on this $hz Hz screen — they only cost bitrate.';
  if (hz % rung == 0) return 'Even on this $hz Hz screen.';
  return 'Uneven on this $hz Hz screen — expect judder. Pick a rate that divides $hz.';
}

(int, int)? parseWh(String v) {
  final p = v.split('x');
  if (p.length != 2) return null;
  final w = int.tryParse(p[0].trim()), h = int.tryParse(p[1].trim());
  return w == null || h == null ? null : (w, h);
}
