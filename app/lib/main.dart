import 'dart:async';
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
  // Under the splash, where holding the main thread shows nothing (`Session.warm`).
  if (failure == null) await Session.I.warm().catchError((_) {});
  runApp(WadoApp(failure: failure));
  // Every frame over budget, with where its time went (build is this app's Dart; raster is
  // drawing) — in Diagnostics' log, so a stutter on a phone can be pointed at. Release builds
  // keep only the freezes (over 100 ms); the timings come batched and cost next to nothing.
  final over = kReleaseMode ? 100 : 24;
  // A frozen screen with no frame to time: this app's Dart held up (a big message, a sync call).
  // ponytail: a 250 ms tick; a finer one costs wake-ups for no better answer.
  final beat = Stopwatch()..start();
  Timer.periodic(const Duration(milliseconds: 250), (_) {
    final late = beat.elapsedMilliseconds - 250;
    beat.reset();
    // Only in front: a phone that froze the app in the background is not a stutter.
    if (late > (kReleaseMode ? 150 : 30) && WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed) rlog('main thread held ${late + 250} ms');
  });
  SchedulerBinding.instance.addTimingsCallback((ts) {
    for (final f in ts) {
      if (f.totalSpan.inMilliseconds > over) rlog('slow frame ${f.totalSpan.inMilliseconds} ms — build ${f.buildDuration.inMilliseconds}, raster ${f.rasterDuration.inMilliseconds}, waited ${f.vsyncOverhead.inMilliseconds}');
    }
  });
  if (failure == null) Inbound.start();
}
