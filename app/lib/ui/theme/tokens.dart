// The web client's design tokens (crates/client/assets/base.css), in Dart. Colour comes only from
// the sixteen base16 slots; every surface and text tone is mixed from them in OKLab exactly as
// the CSS `color-mix(in oklab, …)` does, so a scheme swap is sixteen values.
import 'dart:math' as math;

import 'package:flutter/animation.dart';
import 'package:flutter/painting.dart';

/// `color-mix(in oklab, a, b pct%)`: `pct` of `b` into `a`.
Color mix(Color a, Color b, double pct) {
  final x = _lab(a), y = _lab(b);
  final t = pct / 100;
  final l = x[0] + (y[0] - x[0]) * t;
  final m = x[1] + (y[1] - x[1]) * t;
  final s = x[2] + (y[2] - x[2]) * t;
  final alpha = a.a + (b.a - a.a) * t;
  return _rgb(l, m, s, alpha);
}

/// `color-mix(in oklab, c, transparent pct%)`.
Color fade(Color c, double pct) => c.withValues(alpha: c.a * (1 - pct / 100));

double _lin(double c) => c <= 0.04045 ? c / 12.92 : math.pow((c + 0.055) / 1.055, 2.4).toDouble();
double _gam(double c) => c <= 0.0031308 ? 12.92 * c : 1.055 * math.pow(c, 1 / 2.4) - 0.055;
double _cbrt(double v) => v < 0 ? -math.pow(-v, 1 / 3).toDouble() : math.pow(v, 1 / 3).toDouble();

List<double> _lab(Color c) {
  final r = _lin(c.r), g = _lin(c.g), b = _lin(c.b);
  final l = _cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  final m = _cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  final s = _cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
  ];
}

Color _rgb(double L, double A, double B, double alpha) {
  final l = math.pow(L + 0.3963377774 * A + 0.2158037573 * B, 3).toDouble();
  final m = math.pow(L - 0.1055613458 * A - 0.0638541728 * B, 3).toDouble();
  final s = math.pow(L - 0.0894841775 * A - 1.2914855480 * B, 3).toDouble();
  double ch(double v) => _gam(v).clamp(0.0, 1.0);
  return Color.from(
    alpha: alpha,
    red: ch(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    green: ch(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    blue: ch(-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s),
  );
}

/// The derived tokens of base.css's `:root`.
class Tokens {
  Tokens(this.base, {String accent = '0D', this.radius = 'round', this.motion = 'full'})
      : accent = base[int.parse(accent, radix: 16)];

  /// `--base00` … `--base0F`.
  final List<Color> base;
  final Color accent;
  final String radius;
  final String motion;

  Color get s0 => base[0];
  Color get s1 => mix(base[0], base[5], 6);
  Color get s2 => mix(base[0], base[5], 11);
  Color get s3 => mix(base[0], base[5], 18);
  Color get line => mix(base[0], base[5], 14);
  Color get bright => base[5];
  Color get text => mix(base[5], base[0], 10);
  Color get dim => mix(base[5], base[0], 42);
  Color get onAccent => base[0];
  Color get good => base[0xB];
  Color get warn => base[9];
  Color get bad => base[8];
  static const stage = Color(0xFF000000);

  /// `data-ground`: light when base00's luma is over 140 (theme.js).
  bool get light {
    final c = base[0];
    final lum = 0.2126 * c.r * 255 + 0.7152 * c.g * 255 + 0.0722 * c.b * 255;
    return lum > 140;
  }

  double get r1 => switch (radius) { 'sharp' => 3, 'pill' => 14, _ => 10 };
  double get r2 => switch (radius) { 'sharp' => 4, 'pill' => 22, _ => 16 };
  double get r3 => switch (radius) { 'sharp' => 6, 'pill' => 28, _ => 22 };
  double get r4 => switch (radius) { 'sharp' => 8, 'pill' => 34, _ => 28 };
  static const gap = 10.0;

  bool get reduced => motion == 'reduced';
  bool get still => motion == 'off';
  Duration _d(int full, int red) =>
      Duration(milliseconds: still ? 0 : (reduced ? red : full));
  Duration get dur => _d(220, 120);
  Duration get durSheet => _d(480, 200);
  Duration get durHero => _d(520, 160);
  Duration get durPress => _d(120, 120);
  Duration get stagger => _d(40, 0);
  Duration get durStage => _d(620, 160);
  Duration get durStageOut => _d(520, 160);

  /// `--spring` and `--pop`.
  static const spring = Cubic(.32, .72, 0, 1);
  static const popCurve = Cubic(.34, 1.56, .64, 1);
  Curve get pop => reduced ? spring : popCurve;

  static const display = 'Bricolage Grotesque';
  static const body = 'Figtree';
  static const mono = 'JetBrains Mono';
}

/// The schemes in `assets/schemes.json` (a copy of crates/client/src/schemes.json).
class Scheme {
  const Scheme(this.name, this.hexes);
  final String name;
  final List<String> hexes;
  List<Color> get colors => hexes.map((h) => Color(0xFF000000 | int.parse(h, radix: 16))).toList();
}
