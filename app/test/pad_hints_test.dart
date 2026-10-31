import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/pad_hints.dart';

void main() {
  test('the pad reads the obvious gestures, and nothing else', () {
    const o = Offset(100, 100);
    expect(classify([(o, o)], 200), isNull);
    expect(classify([(o, o)], 900)!.kind, 'hold');
    expect(classify([(o, o + const Offset(20, 0))], 900), isNull);
    final scroll = classify([(o, o + const Offset(0, 40)), (o + const Offset(50, 0), o + const Offset(50, 40))], 100)!;
    expect((scroll.kind, scroll.horizontal), ('scroll', false));
    final pinch = classify([(o, o - const Offset(30, 0)), (o + const Offset(50, 0), o + const Offset(80, 0))], 100)!;
    expect((pinch.kind, pinch.out), ('pinch', true));
    expect(pinch.scale, closeTo(2.2, .01));
    final three = [for (final x in const [0.0, 40.0, 80.0]) (o + Offset(x, 0), o + Offset(x + 60, 0))];
    expect(classify(three, 100)!.label, 'Workspace');
    expect(classify([for (final x in const [0.0, 30.0, 60.0, 90.0]) (o + Offset(x, 0), o + Offset(x, 0))], 50)!.kind, 'four');
  });
}
