import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/autorate.dart';

void main() {
  test('congestion steps down, a clean link climbs back to the full rate', () {
    final a = Autorate()..target(6000);
    var t = 0;
    bool tick(double ping, double kbps) => a.feed(ping: ping, kbps: kbps, lossPct: 0, nowMs: t += 1000);
    for (var i = 0; i < 10; i++) {
      tick(20, 6000);
    }
    expect(a.cap, isNull);
    // Round trip far over its floor for 3 s: down, to ~90% of what got through, at most halving.
    var changed = false;
    for (var i = 0; i < 6 && !changed; i++) {
      changed = tick(200, 1500);
    }
    expect(changed, isTrue);
    expect(a.cap, 3000);
    // Clean for 10 s, a change at most every 6 s: +50% steps, then the cap lifts.
    for (var i = 0; i < 40 && a.cap != null; i++) {
      tick(20, 3000);
    }
    expect(a.cap, isNull);
  });

  test('jitter spikes alone do not move it', () {
    final a = Autorate()..target(6000);
    var t = 0;
    for (var i = 0; i < 30; i++) {
      a.feed(ping: i % 5 == 0 ? 200 : 10, kbps: 6000, lossPct: 0, nowMs: t += 1000);
    }
    expect(a.cap, isNull);
  });
}
