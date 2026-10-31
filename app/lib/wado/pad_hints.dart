// The touchpad's gesture hints (port of js/pad_hints.js `classify`): what the fingers look like
// they are doing — scroll, pinch, a three- or four-finger swipe, four down, one held. Purely a
// picture: the computer's libinput decides what a gesture is; this mirrors the obvious cases.
import 'dart:math' as math;
import 'dart:ui' show Offset;

const holdMs = 350, holdFillMs = 550;
const _still = 8.0, _pinch = 24.0, _scroll = 16.0, _swipe = 24.0;

class PadHint {
  const PadHint(this.kind, this.at, {this.n = 0, this.d = Offset.zero, this.ahead = 0, this.behind = 0, this.out = false, this.scale = 1, this.angle = 0, this.half = 0, this.progress = 0});
  /// scroll · pinch · swipe · four · hold
  final String kind;
  final Offset at, d;
  final int n;
  final double ahead, behind, scale, angle, half, progress;
  final bool out;

  bool get horizontal => d.dx.abs() >= d.dy.abs();

  String get label => switch (kind) {
        'scroll' => 'Scroll',
        'pinch' => 'Pinch ×${scale.toStringAsFixed(1)}',
        'swipe' => n == 3 && horizontal ? 'Workspace' : (n == 3 ? 'Three-finger swipe' : 'Four-finger swipe'),
        'four' => 'Four fingers',
        _ => progress >= 1 ? 'Holding' : '',
      };
}

/// `fingers`: where each landed (`$1`) and is (`$2`); `ms` since this set of fingers came down.
PadHint? classify(List<(Offset, Offset)> f, int ms) {
  final n = f.length;
  if (n == 0 || n > 4) return null;
  final c0 = f.fold(Offset.zero, (a, p) => a + p.$1) / n.toDouble();
  final c = f.fold(Offset.zero, (a, p) => a + p.$2) / n.toDouble();
  final d = c - c0;
  final moved = d.distance;
  if (n == 1) {
    if (moved >= _still || ms < holdMs) return null;
    return PadHint('hold', c, progress: math.min(1, (ms - holdMs) / holdFillMs));
  }
  // How far the fingers reach ahead of and behind their centre along the way they move.
  final u = moved > 0 ? d / moved : const Offset(1, 0);
  final along = [for (final p in f) (p.$2.dx - c.dx) * u.dx + (p.$2.dy - c.dy) * u.dy];
  final ahead = math.max(0.0, along.reduce(math.max)), behind = math.max(0.0, -along.reduce(math.min));
  if (n == 2) {
    final d0 = (f[1].$1 - f[0].$1).distance, dn = (f[1].$2 - f[0].$2).distance;
    final dd = dn - d0;
    if (dd.abs() > _pinch && dd.abs() > moved) {
      final v = f[1].$2 - f[0].$2;
      return PadHint('pinch', c, out: dd > 0, scale: d0 > 0 ? dn / d0 : 1, angle: math.atan2(v.dy, v.dx), half: dn / 2);
    }
    if (moved > _scroll) return PadHint('scroll', c, n: 2, d: d, ahead: ahead, behind: behind);
    return null;
  }
  if (moved > _swipe) return PadHint('swipe', c, n: n, d: d, ahead: ahead, behind: behind);
  return n == 4 ? PadHint('four', c, n: 4) : null;
}
