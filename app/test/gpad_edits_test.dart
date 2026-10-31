import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/gpad_edits.dart';
import 'package:wado/wado/gpad_layouts.dart';

void main() {
  const box = Size(824, 370);
  final l = layoutById('xbox');

  test('a move, a group resize and a mirror land where they should', () {
    final base = place(l, 'l', 1, box);
    var t = moved({}, ['a'], const Offset(40, -20), box);
    expect(placeTweaked(l, 'l', 1, box, t)['a']!.center, base['a']!.center + const Offset(40, -20));

    // The face group grows around its centre: the buttons spread, so none overlap.
    final face = [for (final c in l.controls) if (c.g == 'face') c.id];
    t = sized({}, face, 1.4, base, box);
    final big = placeTweaked(l, 'l', 1, box, t);
    for (final a in face) {
      for (final b in face) {
        if (a != b) expect(big[a]!.overlaps(big[b]!), isFalse, reason: '$a and $b');
      }
    }

    // Never under 44 px, however small it is asked to be.
    final tiny = placeTweaked(l, 'l', 1, box, {'back': const Tweak(s: .2)});
    expect(tiny['back']!.shortestSide, closeTo(44, .01));

    // Mirror twice is where it started.
    final twice = mirrored(l, 'l', 1, box, mirrored(l, 'l', 1, box, t));
    final again = placeTweaked(l, 'l', 1, box, twice);
    for (final id in face) {
      expect((again[id]!.center - big[id]!.center).distance, lessThan(.01));
    }
  });
}
