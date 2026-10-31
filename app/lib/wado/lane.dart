// The fallback lane's hints (port of lane.rs + js/lane_hint.js): where the relay's 64 KB/s cap is
// felt — files on the lane, refused or stalled, a big shell dump — say so at that spot. The rule
// for the words: it is the network, not wado; name the cause when one end could tell, otherwise
// blame nobody; offer the one thing that helps. No picture needs no detector: the stage card
// already says it.
import 'dart:async';

import 'package:flutter/foundation.dart';

import 'link_state.dart';
import 'relay_link.dart';

typedef LaneWords = ({String title, String body, bool retry});

String causeWords(LinkNow l) => l.nat == 'symmetric' && !l.turn
    ? ' Your computer is behind a VPN or a strict NAT, which blocks direct connections — switching its VPN off is the likely fix.'
    : ' Switching off a VPN, or moving to mobile data, usually fixes it.';

/// lane.rs `hint`: `detail` is the computer's own refusal, `stalled`, or nothing.
LaneWords laneWords(String spot, LinkNow link, String detail) {
  final direct = link.state == 'direct';
  final cause = causeWords(link);
  if (spot == 'files') {
    if (detail == 'stalled') {
      return (
        title: 'This transfer has stopped moving',
        body: 'Nothing has arrived for a few seconds. That is the network, not wado — it carries on by itself when the path comes back.${direct ? '' : cause}',
        retry: !direct,
      );
    }
    if (detail.isNotEmpty) return (title: 'That needs a direct connection', body: detail, retry: true);
    return (
      title: "Files are on the relay's limited lane",
      body: 'No direct path, so files move at about 64 KB/s, up to 1 MB each; thumbnails, folders as archives and playback wait for a direct connection. This is the network, not wado.$cause',
      retry: true,
    );
  }
  if (direct) {
    return (
      title: "Big output — arriving at the relay's pace",
      body: "Shells always ride wado's relay, which carries 64 KB/s, so a dump this size takes a moment to arrive.",
      retry: false,
    );
  }
  return (
    title: "Big output — on the relay's limited lane",
    body: 'There is no direct path, so everything rides the relay at 64 KB/s and a dump this size takes a while. This is the network, not wado.$cause',
    retry: true,
  );
}

class Lane {
  Lane._() {
    RelayLink.I.on('pty_output', _shell);
  }
  static final I = Lane._();

  /// spot → detail, for the spots showing a hint.
  final hints = ValueNotifier<Map<String, String>>({});

  void show(String spot, [String detail = '']) {
    if (hints.value[spot] == detail) return;
    hints.value = {...hints.value, spot: detail};
  }

  void clear(String spot) {
    if (!hints.value.containsKey(spot)) return;
    hints.value = {...hints.value}..remove(spot);
  }

  // ── shell: output at the lane's pace, over 40 KB/s for 2 s or more ──
  final _seen = <(int, int)>[];
  final _sw = Stopwatch()..start();
  Timer? _quiet;

  void _shell(Map<String, dynamic> m) {
    if (m['replay'] == true) return;
    final now = _sw.elapsedMilliseconds;
    _seen.add((now, '${m['data'] ?? ''}'.length));
    _seen.removeWhere((s) => now - s.$1 > 3000);
    final span = now - _seen.first.$1;
    final bytes = _seen.fold<int>(0, (a, s) => a + s.$2);
    if (span >= 2000 && bytes / (span / 1000) >= 40 * 1024) show('shell');
    _quiet?.cancel();
    _quiet = Timer(const Duration(seconds: 4), () {
      _seen.clear();
      clear('shell');
    });
  }

  // ── files: on the lane, a lane refusal, or a transfer that has not moved for 8 s ──
  int _running = 0, _movedAt = 0;
  Timer? _stall;

  /// The files link: on the lane or not, and the computer's refusal if one came.
  void files({required bool lane, String refusal = ''}) {
    if (refusal.isNotEmpty) return show('files', refusal);
    if (lane) return show('files');
    if (_running == 0) clear('files');
  }

  /// A transfer started (+1) or finished (-1).
  void transfer(int d, {required bool lane}) {
    _running = (_running + d).clamp(0, 1 << 20);
    moved();
    _stall?.cancel();
    if (_running == 0) {
      if (hints.value['files'] == 'stalled') lane ? show('files') : clear('files');
      return;
    }
    _stall = Timer.periodic(const Duration(seconds: 1), (_) {
      if (_sw.elapsedMilliseconds - _movedAt > 8000) {
        show('files', 'stalled');
      } else if (hints.value['files'] == 'stalled') {
        lane ? show('files') : clear('files');
      }
    });
  }

  void moved() => _movedAt = _sw.elapsedMilliseconds;
}
