// Shells on the computer (port of js/pty.js): per-device terminals that outlive the page. The
// list (`shells`), open / close, input, output (`pty_output`, a `replay` resets), resize.
import 'package:flutter/foundation.dart';
import 'package:xterm/xterm.dart';

import 'relay_link.dart';

const shellKeys = {
  'esc': '\x1b', 'tab': '\t', 'up': '\x1b[A', 'down': '\x1b[B', 'right': '\x1b[C', 'left': '\x1b[D', //
  'home': '\x1b[H', 'end': '\x1b[F', 'pgup': '\x1b[5~', 'pgdn': '\x1b[6~', 'pipe': '|', 'tilde': '~', 'slash': '/', 'dash': '-',
};

class Shells {
  Shells._() {
    final l = RelayLink.I;
    l.on('shells', (m) {
      final list = (m['shells'] as List? ?? []).cast<Map<String, dynamic>>();
      for (final id in [...terms.keys]) {
        if (!list.any((s) => s['id'] == id)) terms.remove(id);
      }
      shells.value = list;
      hosts.value = (m['hosts'] as List? ?? []).map((e) => '$e').toList();
      if (!list.any((s) => s['id'] == active.value)) active.value = list.isEmpty ? 0 : list.last['id'] as int;
    });
    l.on('pty_opened', (m) => active.value = m['id'] as int);
    l.on('pty_output', (m) {
      final id = (m['id'] ?? 0) as int;
      if (id == 0) return;
      final t = term(id);
      if (m['replay'] == true) {
        t.buffer.clear();
        t.setCursor(0, 0);
      }
      t.write((m['data'] ?? '') as String);
    });
    l.on('pty_exit', (m) => terms[m['id']]?.write('\r\n\x1b[2m[exited — close this tab, or open another]\x1b[0m\r\n'));
    l.on('__up', (_) => request());
    l.on('session_error', (m) {
      final why = '${m['message'] ?? ''}';
      if (why.startsWith('no shells:')) refused.value = why.substring(10).trim();
    });
  }
  static final I = Shells._();

  final shells = ValueNotifier<List<Map<String, dynamic>>>([]);
  final hosts = ValueNotifier<List<String>>([]);
  final active = ValueNotifier<int>(0);
  /// The computer said no (not allowed, turned off), in its words.
  final refused = ValueNotifier<String>('');
  final mods = ValueNotifier<(bool, bool)>((false, false)); // ctrl, alt
  final terms = <int, Terminal>{};
  final _size = <int, (int, int)>{};

  Terminal term(int id) => terms.putIfAbsent(id, () {
        final t = Terminal(maxLines: 4000);
        t.onOutput = (data) => RelayLink.I.send({'type': 'pty_input', 'id': id, 'data': withMods(data)});
        t.onResize = (w, h, _, _) {
          if (_size[id] == (w, h)) return;
          _size[id] = (w, h);
          RelayLink.I.send({'type': 'pty_resize', 'id': id, 'cols': w, 'rows': h});
        };
        return t;
      });

  String withMods(String data) {
    final (ctrl, alt) = mods.value;
    if (!ctrl && !alt) return data;
    var out = data;
    if (ctrl && data.length == 1) {
      final c = data.toUpperCase().codeUnitAt(0);
      if (c >= 64 && c <= 95) out = String.fromCharCode(c - 64);
    }
    if (alt) out = '\x1b$out';
    mods.value = (false, false);
    return out;
  }

  void request() => RelayLink.I.send({'type': 'shells_request'});
  void open([String? host]) {
    refused.value = '';
    final s = _size[active.value] ?? (80, 24);
    RelayLink.I.send({'type': 'pty_open', 'cols': s.$1, 'rows': s.$2, 'host': host});
  }

  void close(int id) => RelayLink.I.send({'type': 'pty_close', 'id': id});

  void key(String name) {
    final id = active.value;
    if (id == 0) return;
    if (name == 'ctrl' || name == 'alt') {
      final (c, a) = mods.value;
      mods.value = name == 'ctrl' ? (!c, a) : (c, !a);
      return;
    }
    final seq = shellKeys[name];
    if (seq != null) RelayLink.I.send({'type': 'pty_input', 'id': id, 'data': withMods(seq)});
  }
}
