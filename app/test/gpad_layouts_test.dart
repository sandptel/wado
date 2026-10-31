import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/gpad_layouts.dart';

void main() {
  test('on the reference phone every control sits where it was designed', () {
    final l = layoutById('xbox');
    final r = place(l, 'l', 1, const Size(824, 370));
    expect(r['a']!.center.dx, closeTo(.86 * 824, .5));
    expect(r['a']!.width, 64);
  });

  test('on a wider screen the side clusters keep their distance from their edge', () {
    final l = layoutById('xbox');
    final a = place(l, 'l', 1, const Size(824, 370)), b = place(l, 'l', 1, const Size(1024, 370));
    expect(b['stick-l']!.center.dx, closeTo(a['stick-l']!.center.dx, .5));
    expect(1024 - b['a']!.center.dx, closeTo(824 - a['a']!.center.dx, .5));
    expect(b['guide']!.center.dx, closeTo(512, .5));
  });

  test('every layout has distinct ids and known kinds', () {
    for (final l in layouts) {
      expect({for (final c in l.controls) c.id}.length, l.controls.length, reason: l.id);
      for (final c in l.controls) {
        expect(base.containsKey(c.kind), isTrue, reason: '${l.id}/${c.id}');
      }
    }
  });
}
