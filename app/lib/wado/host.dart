// What the selected computer tells this device over its link (port of js/host.js, sessions.js,
// config.js): its state (sound, media, Wi-Fi…), its settings, and the sessions running on it.
// Host state is polled while someone is looking (`watch`), as the web does.
import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';

import '../core/core_host.dart';
import 'relay_link.dart';
import 'session.dart';

class Host {
  Host._() {
    final l = RelayLink.I;
    l.on('host_state', _state);
    l.on('host_error', (m) => note.value = (m['message'] ?? 'That did not work.') as String);
    l.on('sessions', (m) {
      sessions.value = (m['sessions'] as List? ?? []).cast<Map<String, dynamic>>();
      here.value = l.pool.value.instance;
    });
    l.on('config_state', (m) => config.value = m['state'] as Map<String, dynamic>?);
    l.on('config_rejected', (m) => note.value = (m['message'] ?? '') as String);
    l.on('__up', (_) {
      _gets = _dos = _drop = 0;
      get();
      l.send({'type': 'config_get'});
      sessionsGet();
    });
    l.on('__down', (_) {});
  }
  static final I = Host._();

  final state = ValueNotifier<Map<String, dynamic>?>(null);
  final config = ValueNotifier<Map<String, dynamic>?>(null);
  final sessions = ValueNotifier<List<Map<String, dynamic>>>([]);
  final here = ValueNotifier<String>('');
  final note = ValueNotifier<String>('');
  final _pics = <String, Map<String, dynamic>>{};
  int _gets = 0, _dos = 0, _drop = 0;
  Timer? _timer, _sTimer;
  bool _on = false, _fast = false;

  void clear() {
    state.value = null;
    config.value = null;
    sessions.value = [];
  }

  void _state(Map<String, dynamic> msg) {
    if (_drop > 0) {
      _drop--;
      _gets = (_gets - 1).clamp(0, 1 << 30);
      return;
    }
    if (_dos > 0) {
      _dos--;
    } else {
      _gets = (_gets - 1).clamp(0, 1 << 30);
    }
    final s = msg['state'] as Map<String, dynamic>?;
    for (final p in (s?['media'] as List? ?? []).cast<Map<String, dynamic>>()) {
      final bus = '${p['bus']}';
      final had = _pics[bus] ?? {};
      if (p['art_same'] == true) p['art'] = had['art'];
      if (p['icon_same'] == true) p['icon'] = had['icon'];
      _pics[bus] = {'art': p['art'], 'icon': p['icon']};
    }
    // A poll that says nothing new rebuilds nothing: every listener (home under the session too)
    // used to rebuild on each one, a fresh map every 1.2–3 s.
    if (!_same(s, state.value)) state.value = s;
    // The core keeps only the computer's name and OS from this (`note_host`): handed the whole
    // state — cover art and all — on every poll, it was a synchronous parse on the main thread.
    final k = RelayLink.I.key;
    final info = s == null ? '' : jsonEncode(s['info']);
    if (s != null && k != null && '$k|$info' != _noted) {
      _noted = '$k|$info';
      CoreHost.I.core.noteHost(key: k, stateJson: jsonEncode(s));
    }
    final fast = (s?['media'] as List? ?? []).any((p) => p['playing'] == true);
    if (fast != _fast) {
      _fast = fast;
      _arm();
    }
  }

  String _noted = '';

  /// Deep equality over decoded JSON. Art strings kept from the last poll are the same object,
  /// so they compare at once.
  static bool _same(Object? a, Object? b) {
    if (identical(a, b)) return true;
    if (a is Map && b is Map) {
      if (a.length != b.length) return false;
      for (final e in a.entries) {
        if (!b.containsKey(e.key) || !_same(e.value, b[e.key])) return false;
      }
      return true;
    }
    if (a is List && b is List) {
      if (a.length != b.length) return false;
      for (var i = 0; i < a.length; i++) {
        if (!_same(a[i], b[i])) return false;
      }
      return true;
    }
    return a == b;
  }

  void get() {
    if (!RelayLink.I.up.value || _dos > 0) return;
    _gets++;
    RelayLink.I.send({'type': 'host_get'});
  }

  /// A host action (`HostDo`): volume, media, Wi-Fi… The next state answers it.
  void act(Map<String, dynamic> action) {
    _drop = _gets;
    _dos++;
    RelayLink.I.send({'type': 'host_do', 'action': action});
    Timer(const Duration(seconds: 6), () => _dos = (_dos - 1).clamp(0, 1 << 30));
  }

  void _arm() {
    _timer?.cancel();
    if (_on) {
      final ms = Session.I.on.value ? 3000 : (_fast ? 1200 : 2500);
      _timer = Timer.periodic(Duration(milliseconds: ms), (_) => get());
    }
  }

  /// Poll while someone is looking at the computer.
  void watch(bool on) {
    _on = on;
    if (on) get();
    _arm();
  }

  void sessionsGet() {
    if (RelayLink.I.up.value) RelayLink.I.send({'type': 'sessions_request'});
  }

  void sessionsWatch(bool on) {
    _sTimer?.cancel();
    if (on) {
      sessionsGet();
      _sTimer = Timer.periodic(const Duration(seconds: 5), (_) => sessionsGet());
    }
  }

  /// A setting on the computer (config.js `configSet`).
  bool configSet(String key, Object? value, {bool confirmed = false}) =>
      RelayLink.I.send({'type': 'config_set', 'key': key, 'value': '${value ?? ''}', 'confirmed': confirmed});
}
