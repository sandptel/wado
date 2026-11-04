import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/game_mode.dart';
import 'package:wado/wado/settings.dart';

import 'ux_lib.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(loadUx);
  test('a pad game shadows full screen, the pad and its picture, and keeps what changed', () {
    final p = GameProfile(name: 'Celeste', exec: 'celeste', layout: 'racing', picture: 'sharp', x: true);
    final o = p.overlay(refresh: 120);
    expect(o['cockpit'], true);
    expect(o['console_face'], 'gamepad');
    expect(o['pad_on'], true);
    expect(o['gpad_layout'], 'racing');
    expect(o['fps'], 30);
    expect(o['x_server'], true);
    expect(o['swipe3'], false);
    expect(GameProfile(name: 'A', exec: 'a').overlay(refresh: 144)['fps'], 120, reason: 'the phone\'s rate, capped');
    expect(newGame('Steam', '/usr/bin/steam %U').x, true, reason: 'launchers get the X server');

    final s = Settings.I;
    s.set('fps', 48);
    String? kept;
    s.onOver = (k, v) => kept = k;
    s.overlay(o);
    expect(s.i('fps'), 30, reason: 'the overlay wins while it is up');
    s.set('fps', 24);
    expect(kept, 'fps', reason: 'a change in the game goes to the game');
    s.overlay(const {});
    expect(s.i('fps'), 48, reason: 'the phone\'s own value is untouched');
  });

  test('a profile survives its JSON', () {
    final p = GameProfile(name: 'Hades', exec: 'hades', play: 'controller', calm: false)..kept['scale'] = 2.0;
    final q = GameProfile.fromJson(p.toJson());
    expect((q.play, q.calm, q.kept['scale']), ('controller', false, 2.0));
    expect(q.overlay()['pad_on'], false);
  });
}
