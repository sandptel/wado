// Is each other computer's daemon online? (port of js/online.js): the relay answers
// `GET <relay>/online/<id>` with `{daemons}`. Every 20 s while home shows them.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

class Online {
  Online._();
  static final I = Online._();
  /// `relay|id` → daemons online (-1: the relay could not be reached).
  final map = ValueNotifier<Map<String, int>>({});
  Timer? _t;

  void watch(List<(String, String)> targets) {
    _t?.cancel();
    if (targets.isEmpty) return;
    Future<void> probe() async {
      for (final (relay, id) in targets) {
        final key = '$relay|$id';
        var n = -1;
        try {
          final c = HttpClient()..connectionTimeout = const Duration(seconds: 8);
          final rq = await c.getUrl(Uri.parse('${relay.replaceAll(RegExp(r'/$'), '')}/online/${Uri.encodeComponent(id)}'));
          final rs = await rq.close().timeout(const Duration(seconds: 8));
          final j = jsonDecode(await rs.transform(utf8.decoder).join()) as Map<String, dynamic>;
          n = (j['daemons'] ?? 0) as int;
          c.close();
        } catch (_) {}
        map.value = {...map.value, key: n};
      }
    }

    probe();
    _t = Timer.periodic(const Duration(seconds: 20), (_) => probe());
  }
}
