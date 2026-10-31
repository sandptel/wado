import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/health.dart';

void main() {
  Verdict run(Health h, Sample s, [int n = 10]) {
    late Verdict v;
    for (var i = 0; i < n; i++) {
      v = h.feed(s);
    }
    return v;
  }

  test('loss is the network', () {
    final v = run(Health()..target(6000, 60), const Sample(fps: 60, ping: 20, kbps: 6000, lossPct: 5, decodeDropPct: 0));
    expect((v.state, v.side), ('bad', 'network'));
    expect(v.fix, 'try 30 fps or a smaller resolution');
  });

  test('a slow decoder with everything arriving is the device', () {
    final v = run(Health()..target(6000, 60), const Sample(fps: 40, ping: 20, kbps: 6000, lossPct: 0, dec: 20, decodeDropPct: 0));
    expect((v.state, v.side), ('bad', 'your device'));
  });

  test('nothing arriving and nothing lost is the server, unless it says it sent it', () {
    const starved = Sample(fps: 10, ping: 20, kbps: 500, lossPct: 0);
    expect(run(Health()..target(6000, 60), starved).side, 'the server');
    final h = Health()..target(6000, 60);
    h.sentKbps = 5000;
    expect(run(h, starved).side, 'network');
  });

  test('a verdict waits three ticks before it shows', () {
    final h = Health()..target(6000, 60);
    run(h, const Sample(fps: 60, ping: 20, kbps: 6000, lossPct: 0));
    expect(h.feed(const Sample(fps: 60, ping: 400, kbps: 6000, lossPct: 0)).side, 'healthy');
    expect(h.feed(const Sample(fps: 60, ping: 400, kbps: 6000, lossPct: 0)).side, 'healthy');
    expect(h.feed(const Sample(fps: 60, ping: 400, kbps: 6000, lossPct: 0)).side, 'network');
  });
}
