// The Mac's own apps (`desk_apps`, no session): the list the daemon sends (`apps_list` — which
// run, which is in front, which are hidden, which its Dock keeps and where), kept fresh every
// 3 s while a dock is on screen, and what can be done to each (`app_do`). The list and icons
// themselves ride Apps (apps.dart), shared with the session's drawer.
import 'dart:async';

import 'package:flutter/foundation.dart';

import 'apps.dart';
import 'relay_link.dart';
import 'session.dart';

typedef App = Map<String, dynamic>;

/// The dock: the apps the Mac's Dock keeps, in its order, then those running that it does not.
(List<App>, List<App>) dockGroups(List<App> all) {
  final kept = [for (final a in all) if (a['dock'] is num) a]..sort((a, b) => (a['dock'] as num).compareTo(b['dock'] as num));
  final extra = [for (final a in all) if (a['running'] == true && a['dock'] is! num) a]..sort(_byName);
  return (kept, extra);
}

/// What runs: the one in front first, then as the Dock keeps them, then by name.
List<App> openNow(List<App> all) => [for (final a in all) if (a['running'] == true) a]
  ..sort((a, b) {
    if ((a['front'] == true) != (b['front'] == true)) return a['front'] == true ? -1 : 1;
    final da = a['dock'] is num ? a['dock'] as num : 1 << 20, db = b['dock'] is num ? b['dock'] as num : 1 << 20;
    return da != db ? da.compareTo(db) : _byName(a, b);
  });

int _byName(App a, App b) => '${a['name']}'.toLowerCase().compareTo('${b['name']}'.toLowerCase());

/// In front · Running · Hidden · Not open
String stateOf(App a) => a['running'] != true ? 'Not open' : (a['away'] == true ? 'Hidden' : (a['front'] == true ? 'In front' : 'Running'));

/// A tap: open it, show it again, or bring it to the front.
String tapAction(App a) => a['away'] == true ? 'unhide' : 'open';

/// The long-press menu, as the Mac's Dock has it: (action, label, dangerous).
List<(String, String, bool)> menuFor(App a) {
  final run = a['running'] == true;
  return [
    (run ? (a['away'] == true ? 'unhide' : 'open') : 'open', run ? 'Show' : 'Open', false),
    if (run && a['away'] != true) ('hide', 'Hide', false),
    if (run) ('quit', 'Quit', false),
    if (run) ('force_quit', 'Force Quit', true),
  ];
}

class DeskApps {
  DeskApps._() {
    Session.I.apps.addListener(() => seen(Session.I.apps.value));
  }
  static final I = DeskApps._();

  /// Apps opened from here and not running yet, by exec: they bounce until they are, or 10 s.
  final launching = ValueNotifier<Set<String>>({});
  int _watchers = 0;
  Timer? _poll;

  /// A dock came on screen (or went): the list is asked every 3 s while any is.
  void watch(bool on) {
    _watchers = (_watchers + (on ? 1 : -1)).clamp(0, 99);
    if (_watchers > 0 && _poll == null) {
      Apps.I.request();
      _poll = Timer.periodic(const Duration(seconds: 3), (_) => Apps.I.request());
    } else if (_watchers == 0) {
      _poll?.cancel();
      _poll = null;
    }
  }

  void act(App a, String action) {
    final exec = '${a['exec']}';
    if (!RelayLink.I.send({'type': 'app_do', 'exec': exec, 'action': action})) return;
    if (action == 'open' && a['running'] != true) {
      launching.value = {...launching.value, exec};
      Timer(const Duration(seconds: 10), () => launching.value = {...launching.value}..remove(exec));
    }
  }

  /// A fresh list arrived: what now runs has stopped bouncing.
  void seen(List<App> all) {
    final run = {for (final a in all) if (a['running'] == true) '${a['exec']}'};
    if (launching.value.any(run.contains)) launching.value = launching.value.difference(run);
  }
}
