// What can be done to the computer's files from here beyond a single request: the clipboard
// (Cut or Copy, then Paste in another folder, with the bar staying while you browse), and an
// undo for each move, copy, duplicate and rename. The server says where each item landed
// (`{to: [...]}`), so an undo puts back exactly what was done, even when a clash renamed it.
import 'package:flutter/foundation.dart';

import 'files.dart';

String parentOf(String p) {
  final i = p.lastIndexOf('/');
  return i <= 0 ? '/' : p.substring(0, i);
}

String baseOf(String p) => p.substring(p.lastIndexOf('/') + 1);

/// Items cut or copied, waiting for a Paste.
class FilesClip {
  const FilesClip(this.op, this.paths);
  /// move · copy
  final String op;
  final List<String> paths;
}

final filesClip = ValueNotifier<FilesClip?>(null);

/// One thing done, and how to take it back.
class Undo {
  const Undo(this.label, this.run);
  final String label;
  final Future<void> Function() run;
}

/// How to take back a move or a copy of `paths` that landed at `to`: a move goes back to where
/// each came from (under another name if that one was taken since), a copy goes to the Trash.
/// Pure, so it is tested without a computer: it returns the requests an undo makes.
List<(String, Map<String, dynamic>)> undoPlan(String op, List<String> paths, List<String> to) {
  if (op == 'copy') return [('trash', {'paths': to})];
  final back = <String, List<String>>{};
  for (var i = 0; i < to.length && i < paths.length; i++) {
    (back[parentOf(paths[i])] ??= []).add(to[i]);
  }
  return [for (final e in back.entries) ('move', {'paths': e.value, 'dest': e.key, 'clash': 'rename'})];
}

/// Move or copy `paths` into `dest`; the undo for it. Throws `exists` when `clash` is `fail` and
/// a name is taken — nothing has moved then.
Future<Undo> transfer(String op, List<String> paths, String dest, {String clash = 'fail'}) async {
  final r = await Files.I.req(op, {'paths': paths, 'dest': dest, 'clash': clash});
  final to = ((r['to'] as List?) ?? const []).map((e) => '$e').toList();
  final n = paths.length;
  final what = n == 1 ? baseOf(paths.first) : '$n items';
  return Undo('${op == 'move' ? 'Moved' : 'Copied'} $what', () async {
    for (final (o, args) in undoPlan(op, paths, to)) {
      await Files.I.req(o, args);
    }
  });
}

/// A copy beside each of `paths` (they may be in different folders), named `name (1).ext`.
Future<Undo> duplicate(List<String> paths) async {
  final byDir = <String, List<String>>{};
  for (final p in paths) {
    (byDir[parentOf(p)] ??= []).add(p);
  }
  final undos = [for (final e in byDir.entries) await transfer('copy', e.value, e.key, clash: 'rename')];
  final what = paths.length == 1 ? baseOf(paths.first) : '${paths.length} items';
  return Undo('Duplicated $what', () async {
    for (final u in undos) {
      await u.run();
    }
  });
}

Future<Undo> rename(String path, String to) async {
  await Files.I.req('rename', {'path': path, 'to': to});
  final now = '${parentOf(path) == '/' ? '' : parentOf(path)}/$to';
  return Undo('Renamed to $to', () => Files.I.req('rename', {'path': now, 'to': baseOf(path)}));
}
