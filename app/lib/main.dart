import 'package:flutter/material.dart';

import 'app.dart';
import 'core/core_host.dart';
import 'ui/theme/look.dart';
import 'wado/e2e.dart';
import 'wado/host.dart';
import 'wado/pad.dart';
import 'wado/passkey.dart';
import 'wado/shells.dart';
import 'wado/session.dart';
import 'wado/relay_link.dart';
import 'wado/settings.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await Settings.I.load();
  await Look.I.load();
  Object? failure;
  try {
    await CoreHost.start();
    // The protocol modules register for their messages once.
    Session.I;
    Host.I;
    Shells.I;
    Pad.I;
    Passkey.I;
    final sel = Settings.I.s('selected');
    if (sel.isNotEmpty && CoreHost.I.computer(sel) != null) RelayLink.I.dial(sel);
    e2eHooks();
  } catch (e) {
    failure = e;
  }
  runApp(WadoApp(failure: failure));
}
