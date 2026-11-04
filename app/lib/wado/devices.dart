// Devices, for a phone the computer lets manage them (gate/delegate.rs): the trusted list, the
// check words of this phone's codes waiting for an answer, and a code just made. Polled while
// the page is open — an answer is wanted within two minutes, and a question arrives unasked.
import 'dart:async';

import 'package:flutter/foundation.dart';

import 'host.dart';
import 'relay_link.dart';

typedef DeviceRow = ({String key, String name, String grants, int? until, bool mine, bool me});
typedef PairAsk = ({String typed, String words, String device, int left});
typedef Minted = ({String link, String typed, int until});

class Devices {
  Devices._() {
    final l = RelayLink.I;
    l.on('device_list', (m) {
      list.value = [
        for (final d in (m['devices'] as List? ?? const []).cast<Map<String, dynamic>>())
          (key: '${d['key']}', name: '${d['name']}', grants: '${d['grants'] ?? ''}', until: (d['until'] as num?)?.toInt(), mine: d['mine'] == true, me: d['me'] == true),
      ];
      asks.value = [
        for (final a in (m['asks'] as List? ?? const []).cast<Map<String, dynamic>>())
          (typed: '${a['typed']}', words: '${a['words']}', device: '${a['device']}', left: (a['left'] as num?)?.toInt() ?? 0),
      ];
      error.value = m['error'] as String? ?? '';
    });
    l.on('pair_minted', (m) {
      final e = m['error'] as String?;
      if (e != null && e.isNotEmpty) {
        error.value = e;
        minted.value = null;
      } else {
        minted.value = (link: '${m['link']}', typed: '${m['typed']}', until: (m['until'] as num?)?.toInt() ?? 0);
      }
      busy.value = false;
    });
  }
  static final I = Devices._();

  final list = ValueNotifier<List<DeviceRow>>([]);
  final asks = ValueNotifier<List<PairAsk>>([]);
  final minted = ValueNotifier<Minted?>(null);
  final error = ValueNotifier<String>('');
  final busy = ValueNotifier<bool>(false);
  Timer? _poll;

  /// The computer gave this phone `devices` (only it can).
  static bool get may => (Host.I.config.value?['grants'] as Map?)?['devices'] == true;

  void watch(bool on) {
    _poll?.cancel();
    if (!on) return;
    refresh();
    _poll = Timer.periodic(const Duration(seconds: 2), (_) => refresh());
  }

  void refresh() => RelayLink.I.send({'type': 'devices_request'});
  void signOut(String key) => RelayLink.I.send({'type': 'device_sign_out', 'key': key});
  void answer(String typed, bool yes) => RelayLink.I.send({'type': 'pair_answer', 'typed': typed, 'yes': yes});

  void mint(List<String> grants, int seconds) {
    error.value = '';
    minted.value = null;
    busy.value = true;
    if (!RelayLink.I.send({'type': 'pair_mint', 'grants': grants, 'trust_for_s': seconds})) {
      busy.value = false;
      error.value = 'Not connected to the computer — try again when it is.';
    }
  }
}
