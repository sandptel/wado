import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/presence.dart';

void main() {
  test('a device appears, moves, leaves the picture, and leaves the roster', () {
    final p = Presence.I..clear();
    p.message('{"p":[{"id":7,"name":"Pixel","hue":3,"kind":"pointer","x":0.25,"y":0.5}]}');
    expect(p.ghosts.value[7]!.name, 'Pixel');
    p.message('{"p":[{"id":7,"kind":"touch","x":0.75,"y":0.5}]}');
    expect((p.ghosts.value[7]!.x, p.ghosts.value[7]!.touch), (0.75, true));
    p.message('{"p":[{"id":7}]}');
    expect(p.ghosts.value, isEmpty);
    p.message('{"p":[{"id":8,"kind":"pointer","x":0,"y":0}]}');
    p.roster([{'id': 1}], 1);
    expect(p.ghosts.value, isEmpty);
    p.message('not json');
  });
}
