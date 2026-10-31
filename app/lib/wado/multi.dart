// Several devices in one session (port of js/multi.js + js/linktest.js + sessions.js's
// `sessionAct`; Decision Log 2026-10-30). Join someone's session, take it over, end it, start
// one of your own on a busy computer — and be told, in plain words, when someone else does one of
// those to yours.
//
// Joining never takes a seat: the link re-dials as a joined room on exactly that session's daemon
// (RelayLink.dialView: `control=1&strict=1&instance=…`, then `room_hello{view}`); the daemon
// serves it as one more participant. An older relay or daemon has none of this: Join is not
// offered, and Take over / End go the way they always went (a seat move). Nothing here retries by
// itself: displaced is displaced until a person taps Rejoin.
import 'dart:async';

import 'package:flutter/foundation.dart';

import 'host.dart';
import 'log.dart';
import 'presence.dart';
import 'relay_link.dart';
import 'session.dart';

/// A prompt as a bottom sheet (js/sheet.js): a title and finger-sized buttons.
class Ask {
  const Ask(this.tag, this.title, this.buttons);
  final String tag, title;
  final List<(String label, bool primary, VoidCallback? run)> buttons;
}

class Multi {
  Multi._() {
    final l = RelayLink.I;
    l.on('__up', (_) => _up());
    l.on('session_left', _left);
    l.on('participants', _participants);
    l.on('session_elsewhere', _elsewhere);
    l.on('desks_full', _full);
    l.on('session_stopped', (_) {
      if (l.view != null) {
        note('Session ended for everyone');
        home();
      }
    });
    l.on('link_test_chunk', _chunk);
    l.on('link_test_done', (_) => _testDone());
    l.on('link_verdict', (m) => _finish(m['verdict'] as Map<String, dynamic>?));
    l.on('__up', (_) {
      final k = _afterUp;
      _afterUp = null;
      if (k != null) _act(k);
      Host.I.sessionsGet();
    });
  }
  static final I = Multi._();

  /// Who is in the session this device watches, and which of them is this device.
  final list = ValueNotifier<List<Map<String, dynamic>>>([]);
  final you = ValueNotifier<int>(0);
  /// What is under way: (instance, kind) — join · takeover · end · start.
  final busy = ValueNotifier<(String, String)>(('', ''));
  final testing = ValueNotifier<bool>(false);
  final verdict = ValueNotifier<Map<String, dynamic>?>(null);
  /// A sheet to show (the UI shows it and clears it) and a passing note (a toast).
  final ask = ValueNotifier<Ask?>(null);
  final notes = StreamController<String>.broadcast();

  ({String instance, String kind, Map<String, dynamic>? config, bool asked})? _room;
  String? _afterUp;
  Timer? _deskWait;

  bool get rooms {
    final c = RelayLink.I.caps.value;
    return c.contains('control') && c.contains('room_strict');
  }

  void note(String text) => notes.add(text);

  // ── sessions.js: resume or stop a session, on whichever daemon of the pool runs it ──

  void _act(String kind) {
    if (kind == 'resume') {
      Session.I.resume();
    } else {
      RelayLink.I.send({'type': 'session_stop'});
      Future.delayed(const Duration(milliseconds: 800), Host.I.sessionsGet);
    }
  }

  /// `sessionAct`: here and up → at once; on another daemon → re-dial for it (taking its seat:
  /// a held seat is this person's own session), then act once the link is up.
  void sessionAct(String instance, String kind) {
    final l = RelayLink.I;
    if (instance == l.pool.value.instance && l.up.value) return _act(kind);
    final k = l.key;
    if (k == null) return;
    final rid = l.remoteId;
    if (rid.isNotEmpty) RelayLink.rememberInstance(rid, instance);
    _afterUp = kind;
    if (kind == 'resume') Session.I.prepare();
    l.dial(k, takeover: true);
  }

  /// A new session while this daemon already has one (sessions.rs `new` + `freshDaemon`): leave
  /// its seat and redial with no daemon asked for, so the relay hands over an idle one.
  void startAnother() {
    final l = RelayLink.I;
    final here = l.pool.value.instance;
    if (Host.I.sessions.value.any((x) => x['instance'] == here) && l.key != null) {
      if (l.remoteId.isNotEmpty) RelayLink.rememberInstance(l.remoteId, '');
      l.dial(l.key!, force: true);
    }
    Session.I.start();
  }

  // ── multi.js ──

  /// Back to this device's own seat.
  void home() {
    final l = RelayLink.I;
    final was = l.view != null;
    _room = null;
    list.value = [];
    you.value = 0;
    busy.value = ('', '');
    if (!was || l.key == null) return;
    l.dial(l.key!, force: true);
    Future.delayed(const Duration(milliseconds: 1500), Host.I.sessionsGet);
  }

  /// kind: join · takeover · end.
  void act(String instance, String kind, Map<String, dynamic>? config) {
    if (!rooms) {
      if (kind == 'join') {
        note('This relay is older than joining — update it, or take the session over instead');
        return;
      }
      sessionAct(instance, kind == 'end' ? 'stop' : 'resume');
      return;
    }
    final l = RelayLink.I;
    if (l.key == null) return;
    if (Session.I.on.value) Session.I.dropStream();
    _room = (instance: instance, kind: kind, config: config, asked: false);
    busy.value = (instance, kind);
    l.dialView(l.key!, instance);
  }

  /// The joined room is up: test the link (not for End), then join, take over or end.
  Future<void> _up() async {
    final room = _room;
    if (room == null || RelayLink.I.view == null) return;
    if (room.asked) {
      // Back after a blip with the picture still ours: rejoin, which displaces nobody.
      if (Session.I.on.value) {
        rlog('joined room back — rejoining the session');
        RelayLink.I.send({'type': 'session_join'});
      }
      return;
    }
    _room = (instance: room.instance, kind: room.kind, config: room.config, asked: true);
    if (room.kind != 'end') {
      final ok = await test('join');
      if (_room?.instance != room.instance) return;
      if (!ok) return home();
    }
    if (room.kind == 'join') {
      Session.I.prepare();
      RelayLink.I.send({'type': 'session_join'});
    } else if (room.kind == 'takeover') {
      Session.I.prepare();
      RelayLink.I.send({'type': 'session_takeover', 'config': room.config ?? Session.config()});
    } else {
      RelayLink.I.send({'type': 'session_stop'});
    }
  }

  /// A join, take over or end refused on a joined room: say why, and go home. True: handled.
  bool error(String why) {
    if (_room == null || Session.I.on.value) return false;
    ask.value = Ask('multi', why.isEmpty ? why : why[0].toUpperCase() + why.substring(1), [('OK', true, home)]);
    busy.value = ('', '');
    return true;
  }

  void _left(Map<String, dynamic> m) {
    final by = (m['by'] as String?) ?? '';
    Session.I.dropStream();
    final text = switch (m['reason']) {
      'taken_over' => '${by.isEmpty ? 'Another device' : by} took over this session',
      'ended' => '${by.isEmpty ? 'Someone' : by} ended this session for everyone',
      'opened_here_again' => 'This session was opened again in another tab of this device',
      'stale' => 'Your connection dropped for too long, so you left the session',
      _ => 'You left this session',
    };
    rlog('session_left: ${m['reason']} by ${by.isEmpty ? '?' : by}');
    note(text);
    final inRoom = RelayLink.I.view != null;
    void rejoin() {
      Session.I.prepare();
      RelayLink.I.send({'type': inRoom ? 'session_join' : 'session_rejoin'});
    }

    final canRejoin = m['reason'] == 'taken_over' || m['reason'] == 'stale';
    ask.value = Ask('multi', '$text.', canRejoin ? [('Rejoin', true, rejoin), ('Home', false, home)] : [('OK', true, home)]);
  }

  void _participants(Map<String, dynamic> m) {
    list.value = [for (final p in (m['list'] as List? ?? [])) Map<String, dynamic>.from(p as Map)];
    you.value = (m['you'] ?? 0) as int;
    Presence.I.roster(list.value, you.value);
    final who = (m['who'] ?? '') as String;
    final me = list.value.where((p) => p['id'] == you.value).firstOrNull;
    if (who.isEmpty || (me != null && me['name'] == who && m['event'] != 'left')) return;
    final text = switch (m['event']) {
      'joined' => '$who joined — they see and control this desktop too',
      'left' => '$who left the session',
      'taken_over' => '$who took this session over',
      _ => null,
    };
    if (text != null) note(text);
  }

  /// "Start new": the speed test, then where the new session goes — this daemon if it is idle,
  /// an idle sibling, or one the daemon starts. Ends in an ordinary Start.
  Future<void> start() async {
    if (!RelayLink.I.up.value) return Session.I.start();
    if (!await test('start')) return;
    _deskWait?.cancel();
    _deskWait = Timer(const Duration(seconds: 15), () {
      _deskWait = null;
      busy.value = ('', '');
      Session.I.start();
    });
    busy.value = ('', 'start');
    RelayLink.I.send({'type': 'desk_request'});
  }

  void _elsewhere(Map<String, dynamic> m) {
    if (_deskWait == null) return;
    _deskWait!.cancel();
    _deskWait = null;
    busy.value = ('', '');
    final l = RelayLink.I;
    final inst = (m['instance'] ?? '') as String;
    if (inst.isNotEmpty && inst != l.pool.value.instance && l.key != null) {
      RelayLink.rememberInstance(l.remoteId, inst);
      rlog('new session goes to $inst');
      Session.I.prepare();
      _afterUp = null;
      l.dial(l.key!, force: true);
      return;
    }
    Session.I.start();
  }

  void _full(Map<String, dynamic> m) {
    if (_deskWait == null) return;
    _deskWait!.cancel();
    _deskWait = null;
    busy.value = ('', '');
    ask.value = Ask('multi', (m['message'] as String?) ?? 'All ${m['max']} sessions are in use — end one, or join one.', [('OK', true, null)]);
  }

  // ── linktest.js: pull a burst, time it, let the daemon judge ──

  static const _bytes = 384 * 1024;
  Completer<Map<String, dynamic>?>? _run;
  String _intent = 'join';
  DateTime? _t0, _first, _last;
  int _got = 0;
  Timer? _testTimer;

  /// The speed test, said: the verdict goes to the Sessions section; a refusal or a warning is
  /// asked as a sheet. True: go ahead.
  Future<bool> test(String intent) async {
    final v = await linkTest(intent);
    if (v == null || v['level'] == 'ok') return true;
    final why = ((v['reasons'] as List?)?.firstOrNull ?? 'the link test failed') as String;
    final done = Completer<bool>();
    if (v['level'] == 'refuse') {
      ask.value = Ask('multi', 'Not possible on this link — $why.', [('OK', true, () => done.complete(false))]);
    } else {
      final go = const {'join': 'Join anyway', 'takeover': 'Take over anyway', 'start': 'Start anyway'}[intent] ?? 'Go ahead';
      ask.value = Ask('multi', 'It will work, but $why.', [(go, true, () => done.complete(true)), ('Cancel', false, () => done.complete(false))]);
    }
    return done.future;
  }

  Future<Map<String, dynamic>?> linkTest(String intent) {
    if (_run != null) return _run!.future;
    final run = Completer<Map<String, dynamic>?>();
    _run = run;
    _intent = intent;
    _t0 = DateTime.now();
    _first = _last = null;
    _got = 0;
    testing.value = true;
    _testTimer = Timer(const Duration(seconds: 8), () {
      rlog("speed test: no answer — the computer's wado is older than the test");
      _finish(null);
    });
    if (!RelayLink.I.send({'type': 'link_test', 'bytes': _bytes})) _finish(null);
    return run.future;
  }

  void _chunk(Map<String, dynamic> m) {
    if (_run == null) return;
    final now = DateTime.now();
    // The first chunk's arrival is the round trip (plus one chunk); the rest is throughput.
    if (_first == null) {
      _first = now;
      return;
    }
    _got += ((m['pad'] ?? '') as String).length;
    _last = now;
  }

  void _testDone() {
    if (_run == null) return;
    final now = DateTime.now();
    final span = ((_last ?? now).difference(_first ?? _t0!).inMicroseconds / 1000).clamp(1, double.infinity);
    final down = (_got * 8 / span).round();
    final rtt = (_first ?? now).difference(_t0!).inMilliseconds;
    rlog('speed test: $down kbps down, $rtt ms to first byte');
    RelayLink.I.send({'type': 'link_report', 'down_kbps': down, 'rtt_ms': rtt, 'intent': _intent});
  }

  void _finish(Map<String, dynamic>? v) {
    final run = _run;
    if (run == null) return;
    _testTimer?.cancel();
    _run = null;
    testing.value = false;
    if (v != null) verdict.value = v;
    run.complete(v);
  }

  /// The daemon did not understand `link_test` (older than it): no verdict, at once.
  void abortTest() => _finish(null);
}
