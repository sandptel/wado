import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/pad_haptics.dart';

void main() {
  const size = Size(400, 300);
  late List<String> felt;
  late PadHaptics h;
  setUp(() {
    felt = [];
    h = PadHaptics(felt.add);
  });

  void fingers(List<int> ids, Offset Function(int id) at, int now) {
    for (final id in ids) {
      h.move(id, at(id), now);
    }
  }

  test('a tap clicks; a quick second touch that moves grabs, and lifting drops', () {
    h.down(1, const Offset(100, 100), 0, size);
    h.up(1, 80);
    expect(felt, ['click']);
    h.down(1, const Offset(100, 100), 150, size);
    h.move(1, const Offset(110, 100), 170);
    h.move(1, const Offset(130, 100), 190);
    h.up(1, 400);
    expect(felt, ['click', 'grab', 'drop']);
  });

  test('a slow touch is not a click', () {
    h.down(1, const Offset(100, 100), 0, size);
    h.up(1, 400);
    expect(felt, isEmpty);
  });

  test('two fingers: a tap right-clicks, moving together scrolls, spreading zooms in steps', () {
    h.down(1, const Offset(100, 100), 0, size);
    h.down(2, const Offset(140, 100), 5, size);
    h.up(1, 60);
    h.up(2, 70);
    expect(felt, ['right']);

    felt.clear();
    h.down(1, const Offset(100, 100), 1000, size);
    h.down(2, const Offset(140, 100), 1005, size);
    for (var k = 1; k <= 5; k++) {
      fingers([1, 2], (id) => Offset(id == 1 ? 100 : 140, 100 + 6.0 * k), 1000 + 16 * k);
    }
    expect(felt, ['scroll'], reason: 'one feel as the scroll takes hold, none after');

    felt.clear();
    h.up(1, 1200);
    h.up(2, 1200);
    h.down(1, const Offset(180, 150), 2000, size);
    h.down(2, const Offset(220, 150), 2005, size);
    for (var k = 1; k <= 6; k++) {
      fingers([1, 2], (id) => Offset(id == 1 ? 180 - 10.0 * k : 220 + 10.0 * k, 150), 2000 + 16 * k);
    }
    expect(felt.first, 'zoom');
    expect(felt.length, greaterThan(2), reason: 'a ratchet as the scale grows: $felt');
  });

  test('three fingers up: a swell, then a detent past half way, an undo coming back', () {
    final three = [1, 2, 3];
    for (final id in three) {
      h.down(id, Offset(100.0 + 40 * id, 250), 0, size);
    }
    for (var k = 1; k <= 10; k++) {
      fingers(three, (id) => Offset(100.0 + 40 * id, 250 - 10.0 * k), 16 * k);
    }
    expect(felt, ['swipe', 'detent'], reason: '100 px of a 150 px half-pad passes half way');
    for (var k = 1; k <= 6; k++) {
      fingers(three, (id) => Offset(100.0 + 40 * id, 150 + 10.0 * k), 160 + 16 * k);
    }
    expect(felt, ['swipe', 'detent', 'undo']);
  });

  test('four fingers spreading: a slow swell, not a swipe', () {
    final four = [1, 2, 3, 4];
    Offset at(int id, double r) => switch (id) {
          1 => Offset(200 - r, 150),
          2 => Offset(200 + r, 150),
          3 => Offset(200, 150 - r),
          _ => Offset(200, 150 + r),
        };
    for (final id in four) {
      h.down(id, at(id, 30), 0, size);
    }
    for (var k = 1; k <= 9; k++) {
      fingers(four, (id) => at(id, 30 + 8.0 * k), 16 * k);
    }
    expect(felt.first, 'pinch');
    expect(felt, contains('detent'));
  });

  test('every feel has a pattern', () {
    for (final f in ['click', 'right', 'grab', 'drop', 'scroll', 'zoom', 'swipe', 'pinch', 'detent', 'undo']) {
      expect(padFeels[f], isNotNull, reason: f);
    }
  });
}
