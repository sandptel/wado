// The other devices in a session (port of js/presence.js): where each one's pointer or finger
// is, normalised over the picture, from the daemon's unordered `wado-presence` channel. A device
// with no `kind` has left the picture; the roster (`participants`) drops devices that left.
import 'dart:convert';

import 'package:flutter/foundation.dart';

class Ghost {
  Ghost(this.id, this.name, this.hue, this.x, this.y, this.touch, this.at);
  final Object id;
  final String name;
  final int hue;
  double x, y;
  bool touch;
  int at;
}

class Presence {
  Presence._();
  static final I = Presence._();

  final ghosts = ValueNotifier<Map<Object, Ghost>>({});
  final _sw = Stopwatch()..start();
  int get now => _sw.elapsedMilliseconds;

  void message(String data) {
    Object? m;
    try {
      m = jsonDecode(data);
    } catch (_) {
      return;
    }
    if (m is! Map) return;
    final next = {...ghosts.value};
    for (final p in (m['p'] as List? ?? const []).whereType<Map>()) {
      final id = p['id'] as Object? ?? 0;
      if (p['kind'] == null) {
        next.remove(id);
        continue;
      }
      final x = ((p['x'] ?? 0) as num).toDouble(), y = ((p['y'] ?? 0) as num).toDouble();
      final g = next[id];
      if (g == null) {
        next[id] = Ghost(id, '${p['name'] ?? 'device'}', ((p['hue'] ?? 0) as num).toInt(), x, y, p['kind'] == 'touch', now);
      } else {
        g
          ..x = x
          ..y = y
          ..touch = p['kind'] == 'touch'
          ..at = now;
      }
    }
    ghosts.value = next;
  }

  /// The roster changed: forget anyone no longer in it.
  void roster(List list, Object? you) {
    final ids = {for (final p in list) if (p is Map && p['id'] != you) p['id']};
    final next = {...ghosts.value}..removeWhere((k, _) => !ids.contains(k));
    if (next.length != ghosts.value.length) ghosts.value = next;
  }

  void clear() => ghosts.value = {};
}
