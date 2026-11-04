// The shared experience model (crates/ux, `wado_ux::op`): the rules the app and the web decide the
// same way — Game mode, posters, help rows per form, Continue, trust words, the tour — called
// through the bridge's one `ux` function. A new shared rule is an arm in `wado_ux::op`; nothing
// here changes. Pure and synchronous (microseconds), so a build may read it.
import 'dart:convert';

import '../bridge/api/ux.dart' as bridge;
import 'log.dart';

Object? ux(String op, [Map<String, Object?> input = const {}]) {
  final out = jsonDecode(bridge.ux(op: op, input: jsonEncode(input)));
  if (out is Map && out['error'] != null) {
    rlog('ux $op: ${out['error']}');
    return null;
  }
  return out;
}

int get nowS => DateTime.now().millisecondsSinceEpoch ~/ 1000;
