import 'dart:convert';
// The relay **link** to the selected computer (port of js/relay_link.js + js/seat.js): one seat,
// kept up and redialled by the core; each opened message handed to whoever registered for its
// type. Synthetic types, as the web: "__up" (secure and ready), "__down", "__waiting",
// "__occupied" (refused: in use, a takeover is possible), "__taken_over".
import 'package:flutter/foundation.dart';

import '../core/core_host.dart';
import 'settings.dart';
import 'log.dart';

typedef Handler = void Function(Map<String, dynamic> msg);

class Pool {
  const Pool({this.instance = '', this.size = 0, this.busy = 0, this.assignment = ''});
  final String instance;
  final int size, busy;
  final String assignment;
  String get tag => instance.isEmpty ? '?' : (instance.contains(':') ? '#${instance.split(':').last}' : instance.substring(0, instance.length.clamp(0, 8)));
}

class SeatBusy {
  const SeatBusy(this.holder, this.idle);
  final String holder;
  final bool? idle;
}

class RelayLink {
  RelayLink._();
  static final I = RelayLink._();

  final _handlers = <String, List<Handler>>{};
  String? _room;
  String? key;
  bool takenOver = false;

  /// True between "__up" and the link dropping.
  final up = ValueNotifier<bool>(false);
  /// 0 nothing · 1 reaching the relay · 2 joined, securing · 3 up (`relayPhase`).
  final phase = ValueNotifier<int>(0);
  final phaseNote = ValueNotifier<String>('');
  final pool = ValueNotifier<Pool>(const Pool());
  final caps = ValueNotifier<List<String>>([]);
  final seatBusy = ValueNotifier<SeatBusy?>(null);
  /// The core's words for the link (Diagnostics, the hop bar).
  final words = ValueNotifier<String>('');
  final _boot = <String, String>{};

  /// Register for one message type. Several handlers may listen; each is called in turn.
  void on(String type, Handler fn) => (_handlers[type] ??= []).add(fn);
  void off(String type, Handler fn) => _handlers[type]?.remove(fn);

  void _fire(String type, Map<String, dynamic> msg) {
    for (final h in [...?_handlers[type]]) {
      try {
        h(msg);
      } catch (e) {
        rlog('relay handler $type threw: $e');
      }
    }
  }

  /// The session's daemon when this link is a joined room (`multi.js` `_relayRoom`), else null.
  String? view;

  /// Which daemon of the pool to ask for, per Remote ID (relay_link.js `rememberInstance`):
  /// per-device routing state, never a setting a person chose.
  static String instanceFor(String rid) {
    try {
      return ((jsonDecode(Settings.I.s('instances').isEmpty ? '{}' : Settings.I.s('instances')) as Map)[rid] ?? '') as String;
    } catch (_) {
      return '';
    }
  }

  static void rememberInstance(String rid, String instance) {
    Map m;
    try {
      m = jsonDecode(Settings.I.s('instances').isEmpty ? '{}' : Settings.I.s('instances')) as Map;
    } catch (_) {
      m = {};
    }
    m[rid] = instance;
    Settings.I.set('instances', jsonEncode(m));
  }

  String get remoteId => _rid;
  String get _rid => (CoreHost.I.computer(key ?? '')?.remoteId ?? '').replaceAll(RegExp(r'[\s-]'), '');

  /// Dial the seat on computer `key` (idempotent), keeping it up until [drop].
  bool dial(String key, {bool takeover = false, bool force = false}) {
    if (this.key == key && _room != null && !takeover && !force && view == null) return true;
    drop(silent: true);
    this.key = key;
    view = null;
    takenOver = false;
    final rid = (CoreHost.I.computer(key)?.remoteId ?? '').replaceAll(RegExp(r'[\s-]'), '');
    final id = CoreHost.I.open(key, 'seat', RoomSink(onState: _state, onMsg: _msg, onRelay: _relay), takeover: takeover, instance: instanceFor(rid));
    if (id == null) {
      this.key = null;
      return false;
    }
    _room = id;
    phase.value = 1;
    return true;
  }

  /// Re-dial as a joined room on exactly `instance` (seatless, strict): one more participant.
  bool dialView(String key, String instance) {
    drop(silent: true);
    this.key = key;
    view = instance;
    takenOver = false;
    final id = CoreHost.I.open(key, 'view', RoomSink(onState: _state, onMsg: _msg, onRelay: _relay), instance: instance);
    if (id == null) {
      view = null;
      return false;
    }
    _room = id;
    phase.value = 1;
    return true;
  }

  void drop({bool silent = false}) {
    final r = _room;
    _room = null;
    if (r != null) CoreHost.I.close(r);
    if (up.value) {
      up.value = false;
      _fire('__down', {});
    }
    if (!silent) key = null;
    phase.value = 0;
  }

  /// Redial with takeover=1 — only ever on a person's tap ("Use it here").
  bool takeover() {
    final k = key;
    if (k == null) return false;
    seatBusy.value = null;
    return dial(k, takeover: true);
  }

  /// Send a message to the computer; false if the link is not up.
  bool send(Map<String, dynamic> msg, {bool urgent = true}) {
    final r = _room;
    if (r == null || !up.value) return false;
    return CoreHost.I.send(r, msg, urgent: urgent);
  }

  void _state(Map<String, dynamic> s) {
    words.value = s['words'] as String? ?? '';
    final st = s['state'] as String;
    if (st == 'open') {
      if (!up.value) {
        // A joined room says what it is first (relay_link.js): the daemon serves it as a viewer.
        if (view != null && _room != null) CoreHost.I.send(_room!, {'type': 'room_hello', 'kind': 'view'});
        up.value = true;
        phase.value = 3;
        seatBusy.value = null;
        _fire('__up', {});
      }
      return;
    }
    if (up.value) {
      up.value = false;
      rlog('relay link closed — $st');
      _fire('__down', {});
    }
    switch (st) {
      case 'dialing':
      case 'unreachable':
      case 'reconnecting':
        phase.value = 1;
      case 'securing':
        phase.value = 2;
      case 'refused':
        phaseNote.value = s['words'] as String? ?? '';
        _fire('__refused', s);
      case 'taken_over':
        takenOver = true;
      case 'idle':
        phase.value = 0;
    }
  }

  void _msg(Map<String, dynamic> m) {
    final t = m['type'];
    if (t is String) _fire(t, m);
  }

  void _relay(Map<String, dynamic> m) {
    switch (m['type']) {
      case 'waiting':
        final why = (m['reason'] as String?) ?? 'waiting for the computer';
        phase.value = 1;
        rlog('relay: $why');
        setStatus('relay: $why');
        _fire('__waiting', {...m, 'reason': why});
      case 'taken_over':
        takenOver = true;
        rlog('this desktop was opened on ${m['by'] ?? 'another device'} — not reconnecting');
        _fire('__taken_over', m);
      case 'join_accepted':
        seatBusy.value = null;
        phase.value = 2;
        pool.value = Pool(
          instance: (m['instance_id'] ?? '') as String,
          size: (m['pool_size'] ?? 0) as int,
          busy: (m['pool_busy'] ?? 0) as int,
          assignment: (m['assignment'] ?? '') as String,
        );
        caps.value = ((m['caps'] ?? []) as List).cast<String>();
        final inst = (m['instance_id'] ?? '') as String;
        if (inst.isNotEmpty && view == null && _rid.isNotEmpty) rememberInstance(_rid, inst);
        final boot = (m['boot_id'] ?? '') as String;
        if (boot.isNotEmpty) {
          final was = _boot[inst];
          _boot[inst] = boot;
          if (was != null && was != boot) rlog('this wado daemon restarted since you were last connected — its apps were closed');
        }
        final p = pool.value;
        rlog(p.size > 0
            ? 'relay link up — ${p.assignment == 'reclaimed' ? 'back on my own daemon ' : 'assigned daemon '}${p.tag}, ${p.busy} of ${p.size} session(s) in use'
            : 'relay link up — a daemon is registered for this Remote ID');
      case 'join_denied':
        final reason = (m['reason'] ?? '') as String;
        if (m['takeover'] == true) _fire('__occupied', m);
        if (RegExp('already has an active connection', caseSensitive: false).hasMatch(reason)) {
          final who = (m['holder'] as String?)?.isNotEmpty == true
              ? m['holder'] as String
              : (RegExp(r'in use by ([^)]*)\)').firstMatch(reason)?.group(1) ?? '');
          final idle = m['idle'] is bool ? m['idle'] as bool : null;
          seatBusy.value = SeatBusy(who, idle);
          pool.value = const Pool(assignment: 'refused: pool full');
          phaseNote.value = idle == true ? '' : 'every wado session on this Remote ID is in use';
          rlog('join refused — ${idle == true ? 'held by ${who.isEmpty ? 'another device' : who}, nothing running there' : reason}');
          return;
        }
        phase.value = 1;
        phaseNote.value = reason.isEmpty ? 'no daemon answered for this Remote ID' : reason;
        rlog('join denied: ${reason.isEmpty ? '?' : reason} — will keep trying');
      case 'error':
        _fire('error', m);
    }
  }
}
