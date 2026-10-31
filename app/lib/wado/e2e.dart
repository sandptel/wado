// Debug-only test hooks (scripts/app-e2e-rig.mjs): pair from a link and start a session without
// a person, so the whole path — pairing, seat link, session, picture — runs headless. Ignored in
// release builds.
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/scheduler.dart';

import '../core/core_host.dart';
import 'log.dart';
import 'nav.dart';
import 'relay_link.dart';
import 'pad.dart';
import 'session.dart';
import 'shells.dart';

const _link = String.fromEnvironment('WADO_E2E_LINK');
const _session = bool.fromEnvironment('WADO_E2E_SESSION');
const _page = String.fromEnvironment('WADO_E2E_PAGE');

const _slow = int.fromEnvironment('WADO_E2E_SLOW');

void e2eHooks() {
  // Slow motion, to film transitions on a software-rendered emulator frame by frame.
  if (!kReleaseMode && _slow > 0) timeDilation = _slow.toDouble();
  if (kReleaseMode || _link.isEmpty) return;
  void go(String key) {
    rlog('e2e: open $key');
    Nav.I.openComputer(key);
    if (_page.isNotEmpty) {
      Future.delayed(const Duration(seconds: 4), () {
        Nav.I.openPage(_page);
        if (_page == 'shells') Future.delayed(const Duration(seconds: 2), () => Shells.I.open());
        if (_page == 'input') {
          Pad.I.state.addListener(() => rlog('e2e: pad ${Pad.I.state.value}'));
          Pad.I.rtt.addListener(() => rlog('e2e: pad rtt ${Pad.I.rtt.value} ms bin ${Pad.I.bin}'));
          Pad.I.error.addListener(() => rlog('e2e: pad error ${Pad.I.error.value}'));
          Pad.I.start();
        }
      });
    }
    if (!_session) return;
    void up(Map<String, dynamic> _) {
      RelayLink.I.off('__up', up);
      rlog('e2e: start a session');
      Session.I.start();
    }

    if (RelayLink.I.up.value) {
      Session.I.start();
    } else {
      RelayLink.I.on('__up', up);
    }
  }

  // Reachability from Dart, to tell the platform's networking from the core's.
  final u = Uri.tryParse(Uri.decodeFull(_link).replaceAll(RegExp(r'.*relay='), '').split('&').first);
  if (u != null && u.host.isNotEmpty) {
    Socket.connect(u.host, u.port, timeout: const Duration(seconds: 5))
        .then((s) {
          rlog('e2e: dart reaches ${u.host}:${u.port}');
          s.destroy();
        })
        .catchError((Object e) => rlog('e2e: dart cannot reach ${u.host}:${u.port}: $e'));
  }
  final have = CoreHost.I.computers.value;
  if (have.isNotEmpty) return go(have.first.key);
  final op = CoreHost.I.pair(_link);
  rlog('e2e: pairing (op $op)');
  CoreHost.I.pairSteps.listen((e) {
    if (e['op'] != op) return;
    rlog('e2e: pair ${e['step']} ${e['why'] ?? ''}');
    if (e['step'] == 'done') go(e['key'] as String);
  });
}
