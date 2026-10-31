// Reading the picture's WebRTC stats once a second for as long as it plays (port of js/stats.js):
// the readouts, the control centre's meters, the health verdict and auto bitrate all read this
// one sample. Every fifth second — or at once on an anomaly — it goes to the log, which is what
// a latency report is traced through.
import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';

import 'autorate.dart';
import 'health.dart';
import 'log.dart';
import 'relay_link.dart';
import 'settings.dart';

class Stats {
  Stats._() {
    final l = RelayLink.I;
    l.on('shedding', (m) => judge.divisor = ((m['divisor'] ?? 1) as num).toInt().clamp(1, 1000));
    l.on('sent_kbps', (m) => judge.sentKbps = m['kbps'] is num ? (m['kbps'] as num).toDouble() : null);
  }
  static final I = Stats._();

  final sample = ValueNotifier<Sample?>(null);
  final verdict = ValueNotifier<Verdict>(const Verdict('ok', 'connecting'));
  final judge = Health();
  final rate = Autorate();
  /// The new encoder cap, when auto bitrate moves it (the session applies it).
  void Function(int? kbps)? onCap;
  Timer? _t;
  bool _strain = false;

  /// A session started or was reconfigured: the server's own yardsticks.
  void target(int kbps, int fps) {
    judge.target(kbps, fps);
    rate.target(kbps);
  }

  void stop() {
    _t?.cancel();
    _t = null;
    sample.value = null;
    _strain = false;
  }

  void start(RTCPeerConnection pc) {
    stop();
    num? lastFrames, lastTs, lastBytes, lastByteTs, lastLost, lastPrecv, lastDropped, lastRecv, lastDecT, lastDecF, lastJD, lastJC;
    var tick = 0;
    final clock = Stopwatch()..start();
    num? n(Object? v) => v is num ? v : num.tryParse('${v ?? ''}');
    _t = Timer.periodic(const Duration(seconds: 1), (_) async {
      List<StatsReport> st;
      try {
        st = await pc.getStats();
      } catch (_) {
        return;
      }
      double? fps, ping, jbuf, kbps, jitter, dec, avail;
      num? lost, precv, dropped, recv;
      for (final r in st) {
        final v = r.values;
        if (r.type == 'inbound-rtp' && (v['kind'] ?? v['mediaType']) == 'video') {
          // Our own clock, in ms: the report's timestamp is µs on Android and ms in a browser.
          final ts = clock.elapsedMicroseconds / 1000;
          final fd = n(v['framesDecoded']);
          if (n(v['framesPerSecond']) != null) {
            fps = n(v['framesPerSecond'])!.toDouble();
          } else if (fd != null) {
            if (lastFrames != null && ts > lastTs!) fps = (fd - lastFrames!) * 1000 / (ts - lastTs!);
            lastFrames = fd;
            lastTs = ts;
          }
          lost = n(v['packetsLost']);
          precv = n(v['packetsReceived']);
          if (n(v['jitter']) != null) jitter = n(v['jitter'])! * 1000;
          recv = n(v['framesReceived']);
          dropped = n(v['framesDropped']);
          final b = n(v['bytesReceived']);
          if (b != null) {
            if (lastBytes != null && ts > lastByteTs!) kbps = (b - lastBytes!) * 8 / (ts - lastByteTs!);
            lastBytes = b;
            lastByteTs = ts;
          }
          final jd = n(v['jitterBufferDelay']), jc = n(v['jitterBufferEmittedCount']);
          if (jd != null && jc != null) {
            if (lastJC != null && jc > lastJC!) jbuf = (jd - lastJD!) / (jc - lastJC!) * 1000;
            lastJD = jd;
            lastJC = jc;
          }
          final dt = n(v['totalDecodeTime']);
          if (dt != null && fd != null) {
            if (lastDecT != null && fd > lastDecF!) dec = (dt - lastDecT!) / (fd - lastDecF!) * 1000;
            lastDecT = dt;
            lastDecF = fd;
          }
        } else if (r.type == 'candidate-pair' && (v['nominated'] == true || v['state'] == 'succeeded')) {
          if (n(v['currentRoundTripTime']) != null) ping = n(v['currentRoundTripTime'])! * 1000;
          if (n(v['availableIncomingBitrate']) != null) avail = n(v['availableIncomingBitrate'])! / 1000;
        }
      }
      double? drop, loss;
      if (dropped != null && recv != null && lastDropped != null && lastRecv != null && recv > lastRecv!) {
        drop = (dropped - lastDropped!) / (recv - lastRecv!) * 100;
      }
      if (dropped != null) lastDropped = dropped;
      if (recv != null) lastRecv = recv;
      if (lost != null && precv != null && lastLost != null && lastPrecv != null) {
        final dl = lost - lastLost!, dp = precv - lastPrecv!;
        if (dl + dp > 0) loss = dl / (dl + dp) * 100;
      }
      final lossDelta = lost != null && lastLost != null ? lost - lastLost! : 0;
      if (precv != null) lastPrecv = precv;
      if (lost != null) lastLost = lost;
      final s = Sample(fps: fps, ping: ping, jbuf: jbuf, dec: dec, jitter: jitter, kbps: kbps, lossPct: loss, decodeDropPct: drop, availableKbps: avail);
      sample.value = s;

      final watching = WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
      final down = pc.connectionState != RTCPeerConnectionState.RTCPeerConnectionStateConnected;
      final was = verdict.value;
      final v = judge.feed(s, idle: !watching ? 'not watching' : (down ? 'connecting' : ''));
      verdict.value = v;
      if (v.state != was.state || v.side != was.side) {
        rlog('verdict ${v.state} ${v.side}${v.detail.isEmpty ? '' : ' — ${v.detail}'}${v.fix.isEmpty ? '' : '  (${v.fix})'}');
      }
      // Tell the daemon this device is the bottleneck, so it sheds frames for it (not with Lock FPS).
      final strain = v.side == 'your device' && !Settings.I.b('fps_lock');
      if (strain != _strain && RelayLink.I.send({'type': 'viewer_strain', 'strained': strain})) _strain = strain;

      if (Settings.I.b('auto_bitrate') && rate.feed(ping: ping, kbps: kbps, lossPct: loss, jbuf: jbuf, nowMs: DateTime.now().millisecondsSinceEpoch)) {
        rlog('autorate ${rate.cap == null ? 'full rate ${rate.ceil}' : rate.cap} kbps');
        onCap?.call(rate.cap);
      }

      final bad = (fps != null && fps < 45) || lossDelta > 5 || (ping != null && ping > 250) || (jbuf != null && jbuf > 250);
      if (++tick % 5 == 0 || bad) {
        String f(double? x, int d) => x == null ? '?' : x.toStringAsFixed(d);
        rlog('${bad ? 'ANOMALY ' : 'stats '}fps=${f(fps, 1)} rtt=${f(ping, 0)}ms jbuf=${f(jbuf, 0)}ms kbps=${f(kbps, 0)} lost=${lost ?? '?'} (+$lossDelta) '
            'framesDropped=${dropped ?? '?'} framesReceived=${recv ?? '?'} dec=${f(dec, 2)}ms${watching ? '' : ' vis=hidden'}');
      }
    });
  }
}
