// What the selected computer tells this device over its link (port of js/host.js, sessions.js,
// config.js): its state (sound, media, Wi-Fi…), its settings, and the sessions running on it.
// Host state is polled while someone is looking (`watch`), as the web does.
import 'dart:async';

import 'package:flutter/foundation.dart';

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
    state.value = s;
    final fast = (s?['media'] as List? ?? []).any((p) => p['playing'] == true);
    if (fast != _fast) {
      _fast = fast;
      _arm();
    }
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
