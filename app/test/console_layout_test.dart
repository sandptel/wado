// The console geometry, against layout.rs's own tests (the same numbers).
import 'package:flutter/painting.dart';

import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/console_layout.dart';

void main() {
  ConsoleLayout at(double vw, double vh, Form form, Face face, {double ow = 0, double oh = 0, EdgeInsets inset = EdgeInsets.zero}) =>
      console(vw: vw, vh: vh, form: form, face: face, outW: ow, outH: oh, inset: inset);

  test('portrait rails hug the picture', () {
    final l = at(390, 844, Form.port, Face.pad);
    expect(l.pic.width, closeTo(270, .6));
    expect(l.pic.height, closeTo(584.3, 1));
    expect(l.k, closeTo(.692, .002));
    expect(l.pic.left - l['ws']!.r.right - 6, closeTo(0, .6));
    expect(l['faces']!.r.left - l.pic.right - 8, closeTo(0, .6));
    final strip = l['strip']!.r, deck = l['deck']!.r, nav = l['nav']!.r;
    expect(strip.top > l.pic.bottom && deck.top > strip.bottom && nav.top > deck.bottom, isTrue);
    expect(deck.height >= 88, isTrue);
    expect(l.deckCol, isFalse);
  });

  test('a landscape session in a portrait console puts the faces over the deck', () {
    final l = at(412, 915, Form.port, Face.keys, ow: 2400, oh: 1080);
    final f = l['faces']!.r, deck = l['deck']!.r;
    expect(l.faces, Flow.row);
    expect(f.top >= l['strip']!.r.bottom && deck.top >= f.bottom, isTrue);
    expect(f.top > l['kbd']!.r.bottom, isTrue);
  });

  test('a taller phone gets a bigger deck, not a dead band', () {
    final s = at(390, 860, Form.port, Face.pad, ow: 1080, oh: 2340), t = at(390, 940, Form.port, Face.pad, ow: 1080, oh: 2340);
    expect((s.pic.width - t.pic.width).abs() < .6, isTrue);
    expect(t['deck']!.r.height >= s['deck']!.r.height + 79, isTrue);
  });

  test('landscape has two columns and a band under the picture', () {
    final l = at(844, 390, Form.land, Face.pad);
    expect(l.deckCol, isTrue);
    expect(l['nav']!.r.right <= l.pic.left, isTrue);
    expect(l['deck']!.r.left >= l.pic.right, isTrue);
    expect(l['strip']!.r.top >= l.pic.bottom, isTrue);
    expect(l.k > .6 && l.k < .8, isTrue);
    expect(l.faces, Flow.grid);
  });

  test('portrait gamepad hides the chin and takes its height', () {
    final pad = at(390, 844, Form.port, Face.pad), gp = at(390, 844, Form.port, Face.gamepad);
    expect(gp['strip']!.shown, isFalse);
    expect(gp['deck']!.r.height > pad['deck']!.r.height + 40, isTrue);
  });

  test('desktop frames the picture evenly', () {
    final l = at(1280, 800, Form.desk, Face.quick);
    expect(l.k > .78 && l.k < .86, isTrue);
    expect((l['ws']!.r.left - (1280 - l['deck']!.r.right)).abs() < 1, isTrue);
  });

  test('a joiner letterboxes inside the same room', () {
    final l = at(390, 844, Form.port, Face.pad, ow: 1920, oh: 1080);
    expect(l.pic.width / l.pic.height, closeTo(16 / 9, .01));
    const ch = 390 * 9 / 16;
    expect(l.ty + l.k * (844 - ch) / 2 - l.pic.top, closeTo(0, .01));
    expect(l['deck']!.r.height > 300, isTrue);
  });

  test('nothing overlaps the picture', () {
    for (final (vw, vh, form) in [(390.0, 844.0, Form.port), (844.0, 390.0, Form.land), (820.0, 1180.0, Form.tport), (1180.0, 820.0, Form.tland), (1280.0, 800.0, Form.desk)]) {
      for (final face in Face.values) {
        final l = at(vw, vh, form, Face.resolve(face.name, form));
        for (final e in l.slots.entries) {
          if (!e.value.shown || const {'bezel', 'pic', 'band', 'pad', 'ws'}.contains(e.key)) continue;
          final r = e.value.r;
          if (r.width == 0 || r.height == 0 || e.value.right) continue;
          expect(r.overlaps(l.pic.deflate(.5)), isFalse, reason: '${form.name} ${face.name}: ${e.key} $r over ${l.pic}');
        }
      }
    }
  });
}
