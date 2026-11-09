import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/desk_apps.dart';

App a(String name, {int? dock, bool run = false, bool front = false, bool away = false}) =>
    {'name': name, 'exec': '/Applications/$name.app', 'dock': ?dock, 'running': run, 'front': front, 'away': away};

void main() {
  final all = [a('Notes', dock: 2), a('Safari', dock: 0, run: true), a('Zed', run: true), a('Calculator'), a('Mail', dock: 1, run: true, away: true), a('Terminal', run: true, front: true)];
  List<String> n(List<App> l) => [for (final x in l) '${x['name']}'];

  test('the dock keeps the Mac\'s order, then what runs that it does not', () {
    final (kept, extra) = dockGroups(all);
    expect(n(kept), ['Safari', 'Mail', 'Notes']);
    expect(n(extra), ['Terminal', 'Zed']);
  });
  test('open now: in front first, then the Dock\'s order, then by name', () {
    expect(n(openNow(all)), ['Terminal', 'Safari', 'Mail', 'Zed']);
  });
  test('states and menus follow the app', () {
    expect([for (final x in all) stateOf(x)], ['Not open', 'Running', 'Running', 'Not open', 'Hidden', 'In front']);
    expect([for (final m in menuFor(all[3])) m.$2], ['Open']);
    expect([for (final m in menuFor(all[4])) m.$1], ['unhide', 'quit', 'force_quit'], reason: 'hidden: Show, no Hide');
    expect([for (final m in menuFor(all[1])) m.$2], ['Show', 'Hide', 'Quit', 'Force Quit']);
    expect(menuFor(all[1]).last.$3, isTrue, reason: 'Force Quit is the dangerous one');
    expect(tapAction(all[4]), 'unhide');
  });
}
