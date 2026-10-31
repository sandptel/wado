// Auto bitrate (port of js/autorate.js): cap the encoder to what this link actually carries,
// through a session reconfigure that rebuilds only the encoder.
//
// Round trip against its own recent floor, and loss, decide; the rate that actually arrived sizes
// the step. Not `availableIncomingBitrate`: measured off by up to 76×.
//
//   down  3 congested seconds → ~90% of the best rate that got through, at most halving
//   up    10 clean seconds   → +50%, until the full rate (the cap is then lifted)
//   any change at most every 6 s, so a step can show its effect before the next
import 'dart:math' as math;

class Autorate {
  static const minKbps = 400, downAfter = 3, upAfter = 10, gapMs = 6000;

  /// kbps in force, or null for the full rate.
  int? cap;
  /// The full rate, as the server last reported it uncapped.
  int ceil = 0;
  final _rtts = <double>[], _recent = <double>[], _recv = <double>[];
  int _bad = 0, _good = 0, _last = -gapMs;

  void reset() {
    cap = null;
    ceil = 0;
    _rtts.clear();
    _recent.clear();
    _recv.clear();
    _bad = _good = 0;
    _last = -gapMs;
  }

  /// The server's rate after a start or reconfigure. Uncapped, it is the ceiling.
  void target(int kbps) {
    if (cap == null && kbps > 0) ceil = kbps;
  }

  /// One stats sample a second. Returns true when the cap changed (read [cap]).
  bool feed({double? ping, double? kbps, double? lossPct, double? jbuf, required int nowMs}) {
    if (ceil == 0) return false;
    if (ping != null) {
      _rtts.add(ping);
      if (_rtts.length > 30) _rtts.removeAt(0);
      _recent.add(ping);
      if (_recent.length > 5) _recent.removeAt(0);
    }
    if (kbps != null) {
      _recv.add(kbps);
      if (_recv.length > 5) _recv.removeAt(0);
    }
    final floor = _rtts.isEmpty ? 0.0 : _rtts.reduce(math.min);
    final loss = lossPct ?? 0;
    // The median of the last 5, against a floor of at least 20 ms: single spikes on a jittery
    // link are not capacity.
    final sorted = [..._recent]..sort();
    final mid = sorted.isEmpty ? null : sorted[sorted.length >> 1];
    final congested = (mid != null && mid > math.max(floor, 20) + 60) || loss > 2;
    final clean = !congested && (ping == null || ping <= floor + 25) && loss < .5 && (jbuf == null || jbuf < 80);
    _bad = congested ? _bad + 1 : 0;
    _good = clean ? _good + 1 : 0;
    if (nowMs - _last < gapMs) return false;
    final cur = cap ?? ceil;
    int? next;
    if (_bad >= downAfter) {
      final got = _recv.fold<double>(0, math.max);
      next = math.max(minKbps, math.max(cur * .5, math.min(cur * .7, got * .9)).round());
      if (next >= cur) return false;
    } else if (_good >= upAfter && cap != null) {
      next = (cur * 1.5).round();
      if (next >= ceil * .95) next = null; // back to the full rate
    } else {
      return false;
    }
    cap = next;
    _last = nowMs;
    _bad = _good = 0;
    return true;
  }
}
