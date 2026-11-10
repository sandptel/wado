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

  test('one finger along the edge zone scrolls; a tap or a sideways move there stays the pad\'s', () async {
    final g = PadGestures.I..reset();
    final felt = <String>[];
    g.scrollFelt.stream.listen(felt.add);
    Host.I.config.value = {'trackpad': {'edge_side': 'right', 'edge_width': 32}};
    // In the right zone, along it: held, then claimed as a scroll, never the pointer.
    expect(g.down(1, const Offset(790, 100), s), isFalse);
    expect(g.move(1, const Offset(791, 112), s), isFalse);
    expect(g.edgeScroll.value?.side, 'right');
    expect(g.move(1, const Offset(795, 160), s), isFalse);
    await Future<void>.delayed(Duration.zero);
    expect(felt, ['drop', 'scroll'], reason: 'a tick as it takes hold, a notch after 36 px');
    expect(g.up(1), isFalse);
    expect(g.edgeScroll.value, isNull);
    // A tap in the zone is replayed as a tap.
    expect(g.down(2, const Offset(790, 100), s), isFalse);
    expect(g.up(2), isTrue);
    // Moving inward from the zone is the pointer: released, then it reaches the computer.
    expect(g.down(3, const Offset(790, 100), s), isFalse);
    expect(g.move(3, const Offset(770, 101), s), isFalse);
    expect(g.move(3, const Offset(760, 101), s), isTrue);
    expect(g.up(3), isTrue);
    // Off: nothing held at the edge.
    Host.I.config.value = {'trackpad': {'edge_scroll': false}};
    expect(g.down(4, const Offset(790, 100), s), isTrue);
    expect(g.up(4), isTrue);
  });

  test('the scroll stick: still near its centre, then faster the further it is pushed', () {
    expect(PadGestures.stickSpeed(0), 0);
    expect(PadGestures.stickSpeed(PadGestures.stickDead), 0);
    final slow = PadGestures.stickSpeed(30), fast = PadGestures.stickSpeed(120);
    expect(slow, greaterThan(0));
    expect(fast, greaterThan(slow * 3), reason: 'a curve: fine control near, a page at a push');
    expect(PadGestures.stickSpeed(-120), -fast);
    expect(PadGestures.stickSpeed(1000), PadGestures.stickSpeed(400), reason: 'it tops out');
  });

  test('on a computer with pad-scroll, the edge is a stick held about where it landed', () async {
    final g = PadGestures.I..reset();
    final felt = <String>[];
    g.scrollFelt.stream.listen(felt.add);
    Host.I.config.value = {'caps': ['pad-scroll'], 'trackpad': {'edge_side': 'right'}};
    expect(g.down(1, const Offset(790, 200), s), isFalse);
    expect(g.move(1, const Offset(790, 190), s), isFalse);
    expect(g.edgeScroll.value?.anchor, 200, reason: 'where it landed is the centre');
    expect(g.move(1, const Offset(790, 80), s), isFalse);
    expect(g.edgeScroll.value!.speed, greaterThan(0));
    await Future<void>.delayed(Duration.zero);
    expect(felt.first, 'drop');
    expect(felt.length, greaterThan(1), reason: 'each step up in speed is felt');
    expect(g.up(1), isFalse);
    expect(g.edgeScroll.value, isNull);
  });
}
