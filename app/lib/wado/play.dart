// Opening an app as a game: its Game mode shadows the settings, the session starts (or the running
// one is rejoined), and the app is launched once the session is up — unless it is running there
// already, so a second tap never opens a second window. And the "Continue" history home reads.
import 'dart:async';

import 'package:flutter/foundation.dart';

import 'apps.dart';
import 'game_mode.dart';
import 'host.dart';
import 'multi.dart';
import 'nav.dart';
import 'session.dart';
import 'settings.dart';
import 'ux.dart';

/// One thing done on a computer, for Continue: `game`, `desktop`, `shell`, `file`.
typedef Recent = ({String computer, String kind, String name, String exec, int at});

class Play extends ChangeNotifier {
  Play._() {
    Session.I.on.addListener(_up);
    Session.I.apps.addListener(_listed);
  }
  static final I = Play._();

  String? _exec;
  Timer? _patience;

  /// Open [p]'s app on [computer] in its Game mode.
  void open(String computer, GameProfile p) {
    final switched = Nav.I.selected != computer;
    if (switched) Nav.I.select(computer);
    GameMode.I.begin(computer, p);
    _exec = p.exec;
    note(computer, 'game', p.name, p.exec);
    // As a computer card's ▶ (cards.dart `go`): a fresh link starts; a known running session is
    // rejoined, and the launch then lands in it.
    final mine = switched ? null : _best();
    if (mine == null) {
      Session.I.start();
    } else {
      Multi.I.sessionAct('${mine['instance']}', 'resume');
    }
    if (Session.I.on.value) _up();
  }

  Map<String, dynamic>? _best() {
    final l = Host.I.sessions.value;
    return l.isEmpty ? null : l.first;
  }

  /// The session is up: ask what is running now, then launch unless it already is.
  void _up() {
    if (!Session.I.on.value || _exec == null) return;
    Apps.I.request();
    _patience?.cancel();
    // A big icon list can take seconds through the relay; past that, launch anyway.
    _patience = Timer(const Duration(seconds: 4), _launch);
  }

  void _listed() {
    if (_exec != null && Session.I.on.value && _patience != null) _launch();
  }

  void _launch() {
    _patience?.cancel();
    _patience = null;
    final exec = _exec;
    _exec = null;
    if (exec == null) return;
    final running = Session.I.apps.value.any((a) => a['exec'] == exec && a['running'] == true);
    if (!running) Session.I.launch(exec);
  }

  /// Remember what was done, newest first, a few per computer.
  void note(String computer, String kind, String name, String exec) {
    // `wado_ux::recent::note`: the same thing once, newest first, a few.
    final list = ux('note', {
      'list': Settings.I.get<Object?>('recent_play') ?? const [],
      'recent': {'c': computer, 'k': kind, 'n': name, 'e': exec, 't': nowS},
    });
    if (list is List) Settings.I.set('recent_play', list);
    // Its own signal: home's play section must not rebuild on every setting (a volume slider).
    notifyListeners();
  }

  List<Recent> get recent => [
        for (final m in (Settings.I.get<Object?>('recent_play') as List? ?? const []))
          if (m is Map) (computer: '${m['c']}', kind: '${m['k']}', name: '${m['n']}', exec: '${m['e']}', at: (m['t'] as num?)?.toInt() ?? 0),
      ];

  /// Played recently: one of the last two things done, or anything in three days, was a game.
  bool gamedLately(String computer) =>
      ux('gamed_lately', {'list': Settings.I.get<Object?>('recent_play') ?? const [], 'computer': computer, 'now': nowS}) == true;
}
