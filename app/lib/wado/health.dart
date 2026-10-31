// The one-line verdict: *whose* fault is this? (port of js/health.js)
//
// The three faults look identical from the sofa and have nothing in common underneath:
//   network     — packets lost or delayed between the ends; better signal is the fix
//   your device — the packets arrived, this phone cannot decode them fast enough
//   the server  — nothing is arriving and nothing is lost: the far end never sent it
// The discriminator is loss against throughput, with what the daemon says it sent as the
// tie-breaker (`packetsLost = 0` does not mean no loss).

class Sample {
  const Sample({this.fps, this.ping, this.jbuf, this.dec, this.jitter, this.kbps, this.lossPct, this.decodeDropPct, this.availableKbps});
  final double? fps, ping, jbuf, dec, jitter, kbps, lossPct, decodeDropPct, availableKbps;
}

class Verdict {
  const Verdict(this.state, this.side, {this.detail = '', this.fix = '', this.needKbps, this.gotKbps});
  /// ok · warn · bad
  final String state, side, detail, fix;
  final double? needKbps, gotKbps;
}

class Health {
  static const _warmup = 5, _settle = 3;
  static const _lossWarn = .5, _lossBad = 3.0, _rttWarn = 150, _rttBad = 300, _jitterWarn = 30, _dropWarn = 2.0;

  int targetKbps = 0, targetFps = 0, divisor = 1;
  /// What the daemon says left its socket, or null until it reports (never read as zero).
  double? sentKbps;
  int _warm = 0, _pendingFor = 0;
  Verdict _shown = const Verdict('ok', 'healthy');
  Verdict? _pending;

  /// A session (re)started: the yardsticks are new.
  void target(int kbps, int fps) {
    targetKbps = kbps;
    targetFps = fps;
    _warm = 0;
    _shown = const Verdict('ok', 'healthy');
    _pending = null;
    _pendingFor = 0;
    divisor = 1;
    sentKbps = null;
  }

  static String mbps(double k) => k >= 1000 ? '${(k / 1000).toStringAsFixed(1)} Mbps' : '${k.round()} kbps';

  /// One sample a second. `idle`: the picture is not being watched or the link is down.
  Verdict feed(Sample s, {String idle = ''}) {
    final need = targetKbps > 0 ? targetKbps.toDouble() : null;
    if (_warm++ < _warmup) return Verdict('ok', 'connecting', needKbps: need, gotKbps: s.kbps);
    if (idle.isNotEmpty) {
      _warm = _warmup;
      return Verdict('ok', idle, needKbps: need, gotKbps: s.kbps);
    }
    final effFps = targetFps > 0 ? targetFps / divisor : 0.0;
    final budget = effFps > 0 ? 1000 / effFps : null;
    var state = 'ok', side = 'healthy', detail = '';
    String? cause;
    void worse(String st, String sd, String d, [String? c]) {
      const rank = {'ok': 0, 'warn': 1, 'bad': 2};
      if (rank[st]! > rank[state]!) {
        state = st;
        side = sd;
        detail = d;
        cause = c;
      }
    }

    final loss = s.lossPct, ping = s.ping, got = s.kbps, fps = s.fps;
    if (loss != null && loss >= _lossWarn) worse(loss >= _lossBad ? 'bad' : 'warn', 'network', '${loss.toStringAsFixed(1)}% packet loss');
    if (ping != null && ping >= _rttWarn) worse(ping >= _rttBad ? 'bad' : 'warn', 'network', 'round trip ${ping.toStringAsFixed(0)} ms', 'rtt');
    if (s.jitter != null && s.jitter! >= _jitterWarn) worse('warn', 'network', 'jitter ${s.jitter!.toStringAsFixed(0)} ms');
    // Only judge the decoder when the stream is actually arriving.
    final arriving = got != null && targetKbps > 0 && got >= targetKbps / divisor * .6;
    if (arriving) {
      final drop = s.decodeDropPct;
      if (drop != null && drop >= _dropWarn) worse('warn', 'your device', '${drop.toStringAsFixed(1)}% of frames dropped after arriving');
      final keepingUp = fps != null && effFps > 0 && fps >= effFps * .9 && (drop == null || drop < _dropWarn);
      if (!keepingUp && s.dec != null && budget != null && s.dec! >= budget * .9) {
        worse(s.dec! >= budget ? 'bad' : 'warn', 'your device', 'decode ${s.dec!.toStringAsFixed(1)} ms against a ${budget.toStringAsFixed(1)} ms budget');
      }
    }
    final expect = targetKbps / divisor;
    if (state == 'ok' && loss != null && loss < _lossWarn && got != null && expect > 0 && got < expect * .25 && fps != null && effFps > 0 && fps < effFps * .8) {
      final shed = divisor > 1 ? ' (sending 1 tick in $divisor)' : '';
      final sent = sentKbps;
      if (sent != null && sent > expect * .25) {
        worse('bad', 'network', 'only ${mbps(got)} arriving of ${mbps(sent)} the server actually sent — the path is discarding it silently$shed');
      } else if (sent != null) {
        worse('bad', 'the server', 'the server sent only ${mbps(sent)} of ${mbps(expect)} asked for$shed');
      } else {
        worse('bad', 'the server', 'only ${mbps(got)} arriving of ${mbps(expect)} asked for, none lost (the server has not reported what it sent)$shed');
      }
    }
    final jbufWarn = budget != null ? (budget * 4 > 40 ? budget * 4 : 40) : 40;
    if (s.jbuf != null && s.jbuf! >= jbufWarn) {
      worse('warn', 'settling',
          'the picture is about ${s.jbuf!.toStringAsFixed(0)} ms behind — the player is holding that much playout buffer. It drains when the path steadies, and stays while the round trip does; no setting here shortens it');
    }
    var fix = '';
    if (state != 'ok' && targetFps > 0) {
      final lower = effFps > 60 ? 60 : 30;
      if (side == 'your device') {
        fix = 'try $lower fps';
      } else if (side == 'network' && cause != 'rtt') {
        fix = 'try $lower fps or a smaller resolution';
      }
    }
    // A verdict must hold for 3 ticks before it replaces the one shown: no flicker.
    final v = Verdict(state, side, detail: detail, fix: fix, needKbps: need, gotKbps: got);
    if (state == _shown.state && side == _shown.side) {
      _shown = v;
      _pending = null;
      _pendingFor = 0;
    } else if (_pending != null && _pending!.state == state && _pending!.side == side) {
      if (++_pendingFor >= _settle) {
        _shown = v;
        _pending = null;
        _pendingFor = 0;
      } else {
        _pending = v;
      }
    } else {
      _pending = v;
      _pendingFor = 1;
    }
    return Verdict(_shown.state, _shown.side, detail: _shown.detail, fix: _shown.fix, needKbps: need, gotKbps: got);
  }
}
