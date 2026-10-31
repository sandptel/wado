// What opened the app (port of the old app's MainActivity.route + links/DeepLinks.kt): a pairing
// link (wado://pair?…, the QR's https link) pairs; a door (wado://go/<door>[?key=…], the icon
// shortcuts) opens that place on that computer; a share opens "Send to computer".
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/core_host.dart';
import '../../wado/nav.dart';
import '../../wado/session.dart';
import '../home/cards.dart' show go;
import '../input/pad_screen.dart';
import '../pages/add_computer.dart';
import 'send_sheet.dart';

final rootNav = GlobalKey<NavigatorState>();
const os = MethodChannel('wado/os');

class Inbound {
  static Future<void> start() async {
    if (!Platform.isAndroid) return;
    os.setMethodCallHandler((c) async {
      if (c.method == 'intent') handle(Map<String, dynamic>.from(c.arguments as Map));
    });
    var last = '';
    void follow() {
      final k = Nav.I.selected;
      if (k == last) return;
      last = k;
      remember(CoreHost.I.computers.value.where((c) => c.key == k).firstOrNull);
    }

    Nav.I.view.addListener(follow);
    follow();
    final first = await os.invokeMethod<Map>('intent');
    if (first != null) {
      // After the first frame: the shell (and its navigator) must be up.
      WidgetsBinding.instance.addPostFrameCallback((_) => handle(Map<String, dynamic>.from(first)));
    }
  }

  /// The desktop shortcut follows the last used computer (Shortcut.kt).
  static void remember(Computer? c) {
    if (Platform.isAndroid) os.invokeMethod('shortcut', {'key': c?.key ?? '', 'title': c?.name ?? ''});
  }

  static void handle(Map<String, dynamic> m) {
    if (m['kind'] == 'share') return SendSheet.show(SharePayload.of(m));
    final url = (m['url'] ?? '') as String;
    final u = Uri.tryParse(url);
    if (u == null) return;
    if (u.scheme == 'wado' && u.host == 'go') return door(u.pathSegments.firstOrNull ?? '', u.queryParameters['key'] ?? '');
    if (u.queryParameters['pair']?.isNotEmpty == true) {
      AddComputer.pending = url;
      Nav.I.openPage('add_computer', aboutComputer: false);
      return;
    }
    // A connect link without a code: the computer, if this phone knows it.
    final id = (u.queryParameters['id'] ?? '').replaceAll(RegExp(r'\D'), '');
    final c = CoreHost.I.computers.value.where((c) => c.remoteId.replaceAll(RegExp(r'\D'), '') == id).firstOrNull;
    if (c != null) Nav.I.openComputer(c.key);
  }

  static void door(String name, String key) {
    final list = CoreHost.I.computers.value;
    final c = list.where((c) => c.key == key).firstOrNull ?? list.where((c) => c.key == Nav.I.selected).firstOrNull ?? list.firstOrNull;
    if (c == null) return Nav.I.openPage('add_computer', aboutComputer: false);
    final ctx = rootNav.currentContext;
    switch (name == 'desktop' ? 'session' : name) {
      case 'session':
        Nav.I.openComputer(c.key);
        go(c);
      case 'files':
        Nav.I.openComputer(c.key);
        Nav.I.openPage('files');
      case 'shell':
        Nav.I.openComputer(c.key);
        Nav.I.openPage('shells');
      case 'trackpad' || 'keyboard':
        Nav.I.openComputer(c.key);
        if (ctx != null) openPad(ctx, keyboard: name == 'keyboard');
      case 'gamepad':
        Nav.I.openComputer(c.key);
        if (ctx != null) openGamepad(ctx);
      case 'input':
        Nav.I.openComputer(c.key);
        Nav.I.openPage('input');
      default:
        Nav.I.openComputer(c.key);
    }
    if (Session.I.shown.value && name != 'session') Session.I.leave();
  }
}
