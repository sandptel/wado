import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/host.dart';
import 'package:wado/wado/pad_actions.dart';
import 'package:wado/wado/pad_gestures.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const s = Size(800, 400);

  test('an assigned edge swipe is claimed; anything else reaches the computer', () async {
    final g = PadGestures.I;
    final fired = <String>[];
    g.fired.stream.listen((f) => fired.add(f.icon));
    // Nothing assigned: nothing held.
    expect(g.down(1, const Offset(3, 200), s), isTrue);
    expect(g.up(1), isTrue);

    PadActions.I.list.value = [{'id': 'ws-next', 'name': 'Next workspace', 'icon': 'right'}];
    Host.I.config.value = {'trackpad': {'gestures': {'edge-left': 'ws-next'}}};
    // From the edge, inward: held, then claimed, and never reaches the computer.
    expect(g.down(1, const Offset(3, 200), s), isFalse);
    expect(g.move(1, const Offset(60, 202), s), isFalse);
    await Future<void>.delayed(Duration.zero);
    expect(fired, ['right']);
    expect(g.up(1), isFalse);
    // Mid-pad, and a tap at the edge: theirs.
    expect(g.down(2, const Offset(400, 200), s), isTrue);
    expect(g.up(2), isTrue);
    expect(g.down(3, const Offset(3, 200), s), isFalse);
    await Future<void>.delayed(const Duration(milliseconds: 80));
    expect(g.up(3), isTrue);
    expect(fired.length, 1);
  });
}
