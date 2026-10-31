// Claude Code's sessions on the selected computer (port of js/claude.js + claude.rs; daemon:
// crates/server/src/claude/). Reading the cards needs the `shells` grant only; Allow, opening a
// session's shell and starting a new one are behind the passkey. Deny never is: it can only stop.
import 'package:flutter/foundation.dart';

import 'passkey.dart';
import 'relay_link.dart';

class Claude {
  Claude._() {
    final l = RelayLink.I;
    l.on('claude_cards', (m) => cards.value = Map<String, dynamic>.from((m['state'] ?? {}) as Map));
    l.on('claude_dirs', (m) => dirs.value = Map<String, dynamic>.from((m['dirs'] ?? {}) as Map));
    l.on('__up', (_) => l.send({'type': 'claude_get'}));
    l.on('__down', (_) => cards.value = null);
  }
  static final I = Claude._();

  /// `{sessions: [...], hooks, tmux}`, as the computer last said them; null until it has.
  final cards = ValueNotifier<Map<String, dynamic>?>(null);
  /// The new-session picker's folder: `{path, parent, dirs, recent, error}`.
  final dirs = ValueNotifier<Map<String, dynamic>?>(null);
  /// A session's shell is on its way: the UI opens the shell.
  final shell = ValueNotifier<int>(0);

  bool _send(Map<String, dynamic> m) => RelayLink.I.send(m);

  void answer(String session, String ask, bool allow) {
    void go() => _send({'type': 'claude_answer', 'session': session, 'ask': ask, 'allow': allow});
    allow ? Passkey.I.ask('allow', then: go) : go();
  }

  void open(String session) => Passkey.I.ask('claude', then: () {
        _send({'type': 'claude_open', 'session': session});
        shell.value++;
      });

  void folders(String path) => _send({'type': 'claude_dirs_get', 'path': path});

  void start(String dir) => Passkey.I.ask('new', then: () {
        _send({'type': 'claude_new', 'dir': dir});
        shell.value++;
      });
}

/// The state as words a person reads on a card.
String claudeWords(String state) => switch (state) {
      'working' => 'Working',
      'permission' => 'Needs you',
      'waiting' => 'Waiting for you',
      'done' => 'Done',
      'ended' => 'Ended',
      _ => 'Idle',
    };
