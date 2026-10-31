import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

import 'wado/log.dart';

import 'app.dart';
import 'core/core_host.dart';
import 'ui/share/inbound.dart';
import 'ui/theme/look.dart';
import 'wado/e2e.dart';
import 'wado/host.dart';
import 'wado/clip.dart';
import 'wado/multi.dart';
import 'wado/claude.dart';
import 'wado/input.dart';
import 'wado/link_state.dart';
import 'wado/phone.dart';
import 'wado/push.dart';
import 'wado/lane.dart';
import 'wado/latency.dart';
import 'ui/pages/diagnostics.dart' show readout;
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
    Clip.I;
    Multi.I;
    Claude.I;
    Link.I;
    Phone.I;
    Push.I;
    Lane.I;
    Latency.I.wanted = () => readout('latency');
    Input.I;
    final sel = Settings.I.s('selected');
    if (sel.isNotEmpty && CoreHost.I.computer(sel) != null) RelayLink.I.dial(sel);
    e2eHooks();
  } catch (e) {
    failure = e;
  }
  runApp(WadoApp(failure: failure));
  // Profile and debug builds: every frame over budget, with where its time went (build is this
  // app's Dart; raster is drawing). Release builds never pay for it.
  if (!kReleaseMode) {
    SchedulerBinding.instance.addTimingsCallback((ts) {
      for (final f in ts) {
        if (f.totalSpan.inMilliseconds > 24) rlog('slow frame ${f.totalSpan.inMilliseconds} ms — build ${f.buildDuration.inMilliseconds}, raster ${f.rasterDuration.inMilliseconds}');
      }
    });
  }
  if (failure == null) Inbound.start();
}
