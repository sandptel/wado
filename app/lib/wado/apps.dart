// The computer's applications (apps.js): asked for when the drawer opens — the list says which are
// running now — and kept on this phone per computer, so the drawer opens filled while a fresh one
// is on its way. A big icon set can take seconds through the relay, so the drawer says it is
// asking rather than "no list"; no answer in 30 s asks again, once.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

import 'log.dart';
import 'nav.dart';
import 'relay_link.dart';
import 'session.dart';

class Apps {
  Apps._() {
    RelayLink.I.on('apps_list', (m) {
      _wait?.cancel();
      asking.value = false;
      final key = Nav.I.selected;
      if (key.isNotEmpty) _save(key, m['apps'] as List? ?? const []);
    });
    Nav.I.view.addListener(_follow);
  }
  static final I = Apps._();

  final asking = ValueNotifier(false);
  Timer? _wait;
  String _loaded = '';

  void request({bool again = false}) {
    _follow();
    if (!RelayLink.I.send({'type': 'apps_request'})) return;
    asking.value = true;
    _wait?.cancel();
    _wait = Timer(const Duration(seconds: 30), () {
      rlog('apps_request unanswered after 30 s${again ? '' : ' — asking again'}');
      again ? asking.value = false : request(again: true);
    });
  }

  Future<File> _file(String key) async => File('${(await getApplicationSupportDirectory()).path}/apps-${key.hashCode.toUnsigned(32)}.json');

  /// The kept list for the selected computer, when there is no fresh one yet.
  Future<void> _follow() async {
    final key = Nav.I.selected;
    if (key.isEmpty || key == _loaded) return;
    _loaded = key;
    try {
      final f = await _file(key);
      if (!await f.exists()) return;
      final list = (jsonDecode(await f.readAsString()) as List).cast<Map<String, dynamic>>();
      if (Session.I.apps.value.isEmpty && Nav.I.selected == key) Session.I.apps.value = list;
    } catch (_) {}
  }

  Future<void> _save(String key, List list) async {
    try {
      await (await _file(key)).writeAsString(jsonEncode(list));
    } catch (_) {}
  }
}
