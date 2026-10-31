// The gamepad's layouts (port of js/gpad/variants.js + place.js): each control is a kind, what it
// sends, a label, its group, and its centre and scale in landscape (`l`) and portrait (`p`) on a
// reference phone. Placement re-anchors groups: one in the left third keeps its distance from the
// left edge, the right third from the right, the middle from the centre, so on a bigger screen the
// clusters stay under the thumbs and only the empty middle grows.
import 'dart:math' as math;
import 'dart:ui' show Offset, Rect, Size;

class GControl {
  const GControl(this.id, this.kind, this.bind, this.label, this.g, this.l, this.p, {this.icon, this.word = false, this.dir = 4});
  final String id, kind, bind, label, g;
  final List<double> l, p;
  final String? icon;
  final bool word;
  /// The D-pad's directions: 4, or 8 for fighting games.
  final int dir;
  List<double> at(String o) => o == 'l' ? l : p;
}

class GLayout {
  const GLayout(this.id, this.name, this.sub, this.controls, {this.labels = 'letters', this.shape = 'circle'});
  final String id, name, sub;
  /// letters · colors · shapes
  final String labels;
  /// circle · round
  final String shape;
  final List<GControl> controls;
}

const base = <String, Size>{
  'btn': Size(64, 64), 'small': Size(64, 44), 'bumper': Size(84, 46), 'trig': Size(84, 50), //
  'dpad': Size(150, 150), 'stick': Size(128, 128), 'pedal': Size(96, 170), 'slider': Size(230, 76),
};
const ref = {'l': Size(824, 370), 'p': Size(370, 824)};

GControl _c(String id, String kind, String bind, String label, String g, List<double> l, List<double> p, {String? icon, bool word = false, int dir = 4}) =>
    GControl(id, kind, bind, label, g, l, p, icon: icon, word: word, dir: dir);

List<GControl> _shoulders(List<String> lab) => [
      _c('lt', 'trig', 'lt', lab[0], 'sh-l', [.07, .11, 1], [.13, .39, 1]),
      _c('lb', 'bumper', 'lb', lab[1], 'sh-l', [.19, .11, 1], [.38, .39, 1]),
      _c('rb', 'bumper', 'rb', lab[2], 'sh-r', [.81, .11, 1], [.62, .39, 1]),
      _c('rt', 'trig', 'rt', lab[3], 'sh-r', [.93, .11, 1], [.87, .39, 1]),
    ];

List<GControl> _mids(bool words) => [
      _c('back', 'small', 'back', words ? 'Select' : '', 'mid', [.41, .36, 1], [.3, .5, 1], icon: 'copy', word: words),
      _c('guide', 'small', 'guide', '', 'mid', [.5, .36, 1], [.5, .5, 1], icon: 'house'),
      _c('start', 'small', 'start', words ? 'Start' : '', 'mid', [.59, .36, 1], [.7, .5, 1], icon: 'menu', word: words),
    ];

/// A face cluster: north, west, east, south around a centre.
List<GControl> _face(({String s, String e, String w, String n}) lab, List<double> cl, List<double> cp, [double s = 1]) => [
      _c('y', 'btn', 'y', lab.n, 'face', [cl[0], cl[1] - .19, s], [cp[0], cp[1] - .075, s]),
      _c('x', 'btn', 'x', lab.w, 'face', [cl[0] - .075, cl[1], s], [cp[0] - .175, cp[1], s]),
      _c('b', 'btn', 'b', lab.e, 'face', [cl[0] + .075, cl[1], s], [cp[0] + .175, cp[1], s]),
      _c('a', 'btn', 'a', lab.s, 'face', [cl[0], cl[1] + .19, s], [cp[0], cp[1] + .075, s]),
    ];

final layouts = <GLayout>[
  GLayout('xbox', 'Standard', 'A B X Y · two sticks', labels: 'colors', [
    ..._shoulders(['LT', 'LB', 'RB', 'RT']), ..._mids(false),
    _c('stick-l', 'stick', 'ls', 'L', 'stick-l', [.12, .5, 1], [.25, .87, 1]),
    _c('dpad', 'dpad', 'hat', '', 'dpad', [.3, .76, 1], [.23, .64, 1]),
    _c('stick-r', 'stick', 'rs', 'R', 'stick-r', [.68, .76, 1], [.75, .87, 1]),
    ..._face((s: 'A', e: 'B', w: 'X', n: 'Y'), [.86, .59], [.73, .64]),
  ]),
  GLayout('ps', 'Shapes', '△ ○ ✕ □ · sticks low', labels: 'shapes', [
    ..._shoulders(['L2', 'L1', 'R1', 'R2']), ..._mids(false),
    _c('dpad', 'dpad', 'hat', '', 'dpad', [.13, .5, 1], [.23, .64, 1]),
    _c('stick-l', 'stick', 'ls', 'L', 'stick-l', [.33, .78, 1], [.28, .87, 1]),
    _c('stick-r', 'stick', 'rs', 'R', 'stick-r', [.67, .78, 1], [.72, .87, 1]),
    ..._face((s: '', e: '', w: '', n: ''), [.87, .5], [.73, .64]),
  ]),
  GLayout('nintendo', 'Swapped', 'B A Y X · B below', [
    ..._shoulders(['ZL', 'L', 'R', 'ZR']),
    _c('back', 'small', 'back', '−', 'mid', [.41, .36, 1], [.3, .5, 1], word: true),
    _c('guide', 'small', 'guide', '', 'mid', [.5, .36, 1], [.5, .5, 1], icon: 'house'),
    _c('start', 'small', 'start', '+', 'mid', [.59, .36, 1], [.7, .5, 1], word: true),
    _c('stick-l', 'stick', 'ls', 'L', 'stick-l', [.12, .5, 1], [.25, .64, 1]),
    _c('dpad', 'dpad', 'hat', '', 'dpad', [.3, .76, 1], [.25, .87, 1]),
    _c('stick-r', 'stick', 'rs', 'R', 'stick-r', [.68, .76, 1], [.73, .87, 1]),
    ..._face((s: 'B', e: 'A', w: 'Y', n: 'X'), [.86, .5], [.73, .64]),
  ]),
  GLayout('minimal', 'Minimal', 'D-pad and two', [
    _c('dpad', 'dpad', 'hat', '', 'dpad', [.17, .64, 1.15], [.27, .72, 1.15]),
    _c('b', 'btn', 'b', 'B', 'face', [.73, .72, 1.2], [.62, .78, 1.2]),
    _c('a', 'btn', 'a', 'A', 'face', [.875, .55, 1.35], [.82, .68, 1.35]),
    _c('start', 'small', 'start', '', 'mid', [.5, .84, 1], [.5, .92, 1], icon: 'menu'),
  ]),
  GLayout('racing', 'Racing', 'Steer, gas, brake', shape: 'round', [
    _c('lb', 'bumper', 'lb', 'LB', 'sh-l', [.1, .12, 1], [.15, .4, 1]),
    _c('rb', 'bumper', 'rb', 'RB', 'sh-r', [.9, .12, 1], [.85, .4, 1]),
    _c('start', 'small', 'start', '', 'mid', [.5, .3, 1], [.5, .4, 1], icon: 'menu'),
    _c('steer', 'slider', 'ls', '', 'steer', [.22, .74, 1], [.27, .9, .8]),
    _c('x', 'btn', 'x', 'X', 'face', [.585, .55, .95], [.64, .56, 1]),
    _c('a', 'btn', 'a', 'A', 'face', [.585, .8, .95], [.84, .56, 1]),
    _c('lt', 'pedal', 'lt', 'Brake', 'pedals', [.755, .66, .9], [.59, .76, .85]),
    _c('rt', 'pedal', 'rt', 'Gas', 'pedals', [.905, .6, 1], [.86, .76, 1]),
  ]),
  GLayout('fighting', 'Fighting', 'Six buttons, 8-way', [
    _c('back', 'small', 'back', '', 'mid', [.44, .3, 1], [.38, .45, 1], icon: 'copy'),
    _c('start', 'small', 'start', '', 'mid', [.56, .3, 1], [.62, .45, 1], icon: 'menu'),
    _c('dpad', 'dpad', 'hat', '', 'dpad', [.17, .62, 1.1], [.25, .74, 1.05], dir: 8),
    _c('x', 'btn', 'x', 'X', 'face', [.67, .5, .95], [.58, .66, .9]),
    _c('y', 'btn', 'y', 'Y', 'face', [.79, .44, .95], [.75, .63, .9]),
    _c('rb', 'btn', 'rb', 'RB', 'face', [.91, .44, .95], [.91, .63, .9]),
    _c('a', 'btn', 'a', 'A', 'face', [.65, .75, .95], [.56, .8, .9]),
    _c('b', 'btn', 'b', 'B', 'face', [.77, .69, .95], [.73, .77, .9]),
    _c('rt', 'btn', 'rt', 'RT', 'face', [.89, .69, .95], [.9, .77, .9]),
  ]),
  GLayout('retro', 'Retro', 'D-pad, A B, Start', [
    _c('dpad', 'dpad', 'hat', '', 'dpad', [.16, .58, 1.05], [.25, .66, 1.05]),
    _c('b', 'btn', 'b', 'B', 'face', [.74, .66, 1.15], [.64, .7, 1.15]),
    _c('a', 'btn', 'a', 'A', 'face', [.88, .52, 1.15], [.84, .63, 1.15]),
    _c('back', 'small', 'back', 'Select', 'mid', [.42, .84, 1.1], [.37, .87, 1.1], word: true),
    _c('start', 'small', 'start', 'Start', 'mid', [.58, .84, 1.1], [.63, .87, 1.1], word: true),
  ]),
];

GLayout layoutById(String id) => layouts.firstWhere((l) => l.id == id, orElse: () => layouts.first);

String orient(Size s) => s.width >= s.height ? 'l' : 'p';

double _anchor(double f, double d, double a, double k) => f < 1 / 3 ? f * d * k : (f > 2 / 3 ? a - (1 - f) * d * k : a / 2 + (f - .5) * d * k);

/// Every control's box in a `box`-sized area, at scale `k` (1 on the play screen).
Map<String, Rect> place(GLayout l, String o, double k, Size box) {
  final r = ref[o]!;
  final groups = <String, List<double>>{};
  for (final g in {for (final c in l.controls) c.g}) {
    final ms = [for (final c in l.controls) if (c.g == g) c.at(o)];
    groups[g] = [ms.fold<double>(0, (a, p) => a + p[0]) / ms.length, ms.fold<double>(0, (a, p) => a + p[1]) / ms.length];
  }
  return {
    for (final c in l.controls)
      c.id: () {
        final p = c.at(o), gc = groups[c.g]!;
        final cx = _anchor(gc[0], r.width, box.width, k) + (p[0] - gc[0]) * r.width * k;
        final cy = _anchor(gc[1], r.height, box.height, k) + (p[1] - gc[1]) * r.height * k;
        final b = base[c.kind] ?? base['btn']!;
        final s = p.length > 2 ? p[2] : 1.0;
        return Rect.fromCenter(center: Offset(cx, cy), width: b.width * s * k, height: b.height * s * k);
      }(),
  };
}

/// The `k` that fits a whole layout in a thumbnail of `box`.
double thumbK(String o, Size box) => math.min(box.width / ref[o]!.width, box.height / ref[o]!.height);
