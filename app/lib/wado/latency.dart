// Latency, by stage (port of js/latency.js): what a finger feels — a ping on the input channel,
// answered straight back by the daemon — and, while the Latency readout is on, the daemon's own
// per-stage timings (capture, encode, queue, tick, dropped) beside this phone's network, buffer
// and decode. Never summed: the server's legs and this phone's are on unsynchronised clocks.
import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';

import 'log.dart';
import 'relay_link.dart';
import 'stats.dart';

class Latency {
  Latency._() {
    RelayLink.I.on('timing', (m) => timing.value = (m['timings'] as Map?)?.cast<String, dynamic>());
  }
  static final I = Latency._();

  /// Input round trip, ms, smoothed.
  final inputMs = ValueNotifier<double?>(null);
  /// The daemon's stage timings, the last answer.
  final timing = ValueNotifier<Map<String, dynamic>?>(null);
  /// Whether the breakdown is shown (the readout switch); the daemon is only asked then.
  bool Function() wanted = () => false;

  Timer? _t;
  int _seq = 0;
  final _inflight = <int, int>{};
  final _sw = Stopwatch()..start();

  /// The picture's input channel is open: answer its pongs and ping it once a second.
  void attach(RTCDataChannel dc) {
    stop();
    dc.onMessage = (m) {
      try {
        final j = jsonDecode(m.text);
        if (j is Map && j['t'] == 'pong') {
          final sent = _inflight.remove(j['seq']);
          if (sent != null) {
            final rtt = (_sw.elapsedMicroseconds - sent) / 1000;
            inputMs.value = inputMs.value == null ? rtt : inputMs.value! * .7 + rtt * .3;
          }
        }
      } catch (_) {}
    };
    var tick = 0;
    _t = Timer.periodic(const Duration(seconds: 1), (_) {
      if (dc.state == RTCDataChannelState.RTCDataChannelOpen) {
        final seq = ++_seq & 0xffff;
        _inflight[seq] = _sw.elapsedMicroseconds;
        if (_inflight.length > 16) _inflight.remove(_inflight.keys.first);
        try {
          dc.send(RTCDataChannelMessage(jsonEncode({'t': 'ping', 'seq': seq})));
        } catch (_) {}
      }
      if (wanted()) {
        RelayLink.I.send({'type': 'timing_request'});
        if (++tick % 5 == 0) _log();
      }
    });
  }

  void stop() {
    _t?.cancel();
    _t = null;
    _inflight.clear();
    inputMs.value = null;
    timing.value = null;
  }

  void _log() {
    final s = timing.value, x = Stats.I.sample.value;
    String n(Object? v) => v is num ? v.toStringAsFixed(1) : '?';
    rlog('latency capture=${n(s?['capture_ms'])} encode=${n(s?['encode_ms'])} queue=${n(s?['queue_ms'])} tick=${n(s?['tick_ms'])} '
        'net~${n(x?.ping == null ? null : x!.ping! / 2)} buf=${n(x?.jbuf)} decode=${n(x?.dec)} input(rt)=${n(inputMs.value)}');
  }
}
