// The servers a direct path gathers with: the computer's answer to `ice_get` — STUN, and TURN
// with credentials it minted, the one way through when both ends sit behind a VPN — kept until
// they near expiry. Plain STUN when the computer does not answer in time (an older daemon).
import 'dart:async';

import 'relay_link.dart';

const _stun = [
  {'urls': 'stun:stun.l.google.com:19302'},
  {'urls': 'stun:stun.cloudflare.com:3478'},
];

class Ice {
  Ice._() {
    final l = RelayLink.I;
    l.on('ice', (m) {
      final list = (m['servers'] as List? ?? []).whereType<Map>().map((s) => Map<String, dynamic>.from(s)).toList();
      final ttl = (m['ttl'] as num?)?.toInt() ?? 0;
      if (list.isNotEmpty) {
        _got = list;
        // Renewed a little early, so a connection never starts on credentials about to lapse.
        _until = DateTime.now().add(Duration(seconds: (ttl * 0.8).round()));
      }
      final w = _asking;
      _asking = null;
      w?.complete();
    });
    // Another computer, or the same one back: ask again.
    l.on('__up', (_) => _until = DateTime(0));
  }
  static final I = Ice._();

  List<Map<String, dynamic>>? _got;
  DateTime _until = DateTime(0);
  Completer<void>? _asking;

  /// For `createPeerConnection({'iceServers': …})`.
  Future<List<Map<String, dynamic>>> servers() async {
    if (_got != null && DateTime.now().isBefore(_until)) return _got!;
    var w = _asking;
    if (w == null) {
      w = _asking = Completer<void>();
      if (!RelayLink.I.send({'type': 'ice_get'})) _asking = null;
    }
    await w.future.timeout(const Duration(milliseconds: 1500), onTimeout: () {});
    return _got != null && DateTime.now().isBefore(_until) ? _got! : _stun;
  }
}
