// The computer's applications (apps.js): asked for when the drawer opens — the list says which are
// running now — and kept on this phone per computer, so the drawer opens filled while a fresh one
// is on its way.
//
// Lean since 2026-11-05: the list comes without its icons (a few KB, at once), and the icons this
// phone has not kept are asked for 24 at a time. With every icon inline the list was a megabyte,
// held to the relay's lane for tens of seconds — the drawer said "Asking…" and everything behind it
// waited. An older daemon ignores `lean` and sends them inline, which is merged the same way.
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
      final list = (m['apps'] as List? ?? const []).cast<Map<String, dynamic>>();
      var gotIcons = false;
      for (final a in list) {
        final i = a['icon'];
        if (i is String && i.isNotEmpty) gotIcons = _icons['${a['exec']}'] != i || gotIcons;
        if (i is String && i.isNotEmpty) _icons['${a['exec']}'] = i;
      }
      // A refresh that changed nothing (the Mac's dock asks every 3 s) changes nothing on screen.
      final shape = jsonEncode([for (final a in list) {...a}..remove('icon')]);
      if (shape != _shape || gotIcons) {
        _shape = shape;
        _show(list);
        if (key.isNotEmpty) _save(key);
      }
      // What runs and what the Dock keeps first: those are on screen.
      final want = [for (final a in list) if (a['hidden'] != true && !_icons.containsKey('${a['exec']}') && !_asked.contains('${a['exec']}')) a]
        ..sort((a, b) => _rank(a).compareTo(_rank(b)));
      _missing = [..._missing.where((e) => !_icons.containsKey(e)), for (final a in want) if (!_missing.contains('${a['exec']}')) '${a['exec']}'];
      if (_inFlight == 0) _ask();
    });
    RelayLink.I.on('apps_icons_list', (m) {
      _inFlight = 0;
      (m['icons'] as Map? ?? const {}).forEach((k, v) => _icons['$k'] = '$v');
      _show(Session.I.apps.value);
      final key = Nav.I.selected;
      if (key.isNotEmpty) _save(key);
      _ask();
    });
    Nav.I.view.addListener(_follow);
  }
  static final I = Apps._();

  final asking = ValueNotifier(false);
  Timer? _wait;
  String _loaded = '';
  /// Icons kept by `exec`, across lists and restarts.
  final _icons = <String, String>{};
  List<String> _missing = const [];

  void request({bool again = false}) {
    _follow();
    if (!RelayLink.I.send({'type': 'apps_request', 'lean': true})) return;
    asking.value = true;
    _wait?.cancel();
    _wait = Timer(const Duration(seconds: 30), () {
      rlog('apps_request unanswered after 30 s${again ? '' : ' — asking again'}');
      again ? asking.value = false : request(again: true);
    });
  }

  /// The next few icons this phone lacks; one ask at a time, so the lane never fills, and an
  /// icon asked for is never asked again while its answer is on the way.
  void _ask() {
    if (_missing.isEmpty || _inFlight > 0) return;
    final batch = _missing.take(24).toList();
    _missing = _missing.skip(24).toList();
    _asked.addAll(batch);
    if (RelayLink.I.send({'type': 'apps_icons', 'execs': batch}, urgent: false)) {
      _inFlight = batch.length;
      // An answer that never comes does not stop the rest.
      Timer(const Duration(seconds: 20), () {
        if (_inFlight > 0) {
          _inFlight = 0;
          _ask();
        }
      });
    }
  }

  /// Running first, then the Dock's order, then the rest.
  static int _rank(Map<String, dynamic> a) => a['running'] == true ? 0 : (a['dock'] is num ? 1 + (a['dock'] as num).toInt() : 1000);
  final _asked = <String>{};
  int _inFlight = 0;
  String _shape = '';

  /// The list, each app with the icon kept for it.
  void _show(List<Map<String, dynamic>> list) {
    Session.I.apps.value = [
      for (final a in list) (a['icon'] == null && _icons.containsKey('${a['exec']}')) ? {...a, 'icon': _icons['${a['exec']}']} : a,
    ];
  }

  Future<File> _file(String key) async => File('${(await getApplicationSupportDirectory()).path}/apps-${key.hashCode.toUnsigned(32)}.json');

  /// The kept list for the selected computer, when there is no fresh one yet.
  Future<void> _follow() async {
    final key = Nav.I.selected;
    if (key.isEmpty || key == _loaded) return;
    _loaded = key;
    // Another computer: its own icons to ask for.
    _asked.clear();
    _shape = '';
    try {
      final f = await _file(key);
      if (!await f.exists()) return;
      final list = (jsonDecode(await f.readAsString()) as List).cast<Map<String, dynamic>>();
      for (final a in list) {
        final i = a['icon'];
        if (i is String && i.isNotEmpty) _icons['${a['exec']}'] = i;
      }
      if (Session.I.apps.value.isEmpty && Nav.I.selected == key) Session.I.apps.value = list;
    } catch (_) {}
  }

  Future<void> _save(String key) async {
    try {
      await (await _file(key)).writeAsString(jsonEncode(Session.I.apps.value));
    } catch (_) {}
  }
}
