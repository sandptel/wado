// Debug-only test hooks (scripts/app-e2e-rig.mjs): pair from a link and start a session without
// a person, so the whole path — pairing, seat link, session, picture — runs headless. Ignored in
// release builds.
import 'package:flutter/foundation.dart';

import '../core/core_host.dart';
import 'log.dart';
import 'nav.dart';
import 'relay_link.dart';
import 'session.dart';

const _link = String.fromEnvironment('WADO_E2E_LINK');
const _session = bool.fromEnvironment('WADO_E2E_SESSION');

void e2eHooks() {
  if (kReleaseMode || _link.isEmpty) return;
  void go(String key) {
    rlog('e2e: open $key');
    Nav.I.openComputer(key);
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
