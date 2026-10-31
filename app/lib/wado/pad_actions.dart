// The touchpad's quick actions (port of pad_actions.js): what the computer offers to run from the
// pad — previous and next workspace, full screen, play/pause… — asked for over the pad link
// (`actions` → `action_list`), run by id (`run` → `ran`). An empty list followed by a refusal is
// said as the reason, not as "no actions".
import 'dart:async';

import 'package:flutter/foundation.dart';

import 'pad.dart';

class PadActions {
  PadActions._() {
    Pad.I.onMsg((m) {
      switch (m['k']) {
        case 'action_list':
          list.value = [for (final a in (m['actions'] as List? ?? const []).whereType<Map>()) if (a['id'] != null) a.cast<String, dynamic>()];
          _expectRefusal = list.value.isEmpty;
          if (list.value.isNotEmpty) denied.value = '';
        case 'ran':
          final q = _waiting['${m['id']}'];
          if (q != null && q.isNotEmpty) {
            final c = q.removeAt(0);
            if (!c.isCompleted) c.complete((ok: m['ok'] == true, message: '${m['message'] ?? ''}'));
          }
        case 'err' when m['fatal'] != true && _expectRefusal:
          _expectRefusal = false;
          denied.value = '${m['message'] ?? ''}';
        case 'pong':
          break;
        default:
          _expectRefusal = false;
      }
    });
    Pad.I.state.addListener(() {
      if (Pad.I.state.value != 'off') refresh();
    });
  }
  static final I = PadActions._();

  /// [{id, name, icon}]
  final list = ValueNotifier<List<Map<String, dynamic>>>([]);
  final denied = ValueNotifier('');
  bool _expectRefusal = false;
  final _waiting = <String, List<Completer<({bool ok, String message})>>>{};

  void refresh() => Pad.I.send({'k': 'actions'});

  Map<String, dynamic>? byId(String id) => list.value.where((a) => a['id'] == id).firstOrNull;

  Future<({bool ok, String message})> run(String id) {
    final c = Completer<({bool ok, String message})>();
    (_waiting[id] ??= []).add(c);
    if (!Pad.I.send({'k': 'run', 'id': id})) {
      _waiting[id]!.remove(c);
      return Future.value((ok: false, message: 'Not connected to the computer.'));
    }
    return c.future.timeout(const Duration(seconds: 8), onTimeout: () {
      _waiting[id]?.remove(c);
      return (ok: false, message: "The computer didn't answer.");
    });
  }
}
