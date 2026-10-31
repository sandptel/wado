// Home widgets, as data (port of widget.rs): what kinds there are, what each needs, and the
// list's edits. A widget is one control bound to one computer (`host` is its key, so it follows
// a rename). The list is this phone's own setting; its order is the list's order.
import 'dart:convert';

import 'host.dart';
import 'nav.dart';
import 'settings.dart';
import 'side_links.dart';

enum WidgetKind {
  media('Media player', 'Media', 'music', 'host'),
  volume('Volume', 'Volume', 'sound', 'host'),
  brightness('Brightness', 'Brightness', 'sun', 'host'),
  clipboard('Clipboard', 'Clipboard', 'clip', 'clipboard'),
  trackpad('Virtual input', 'Virtual input', 'trackpad', 'input'),
  claude('Claude', 'Claude', 'term', 'shells');

  const WidgetKind(this.label, this.short, this.icon, this.needs);
  final String label, short, icon, needs;
}

class HomeWidget {
  const HomeWidget(this.id, this.kind, this.host);
  final String id, host;
  final WidgetKind kind;
  Map<String, dynamic> toJson() => {'id': id, 'kind': kind.name, 'host': host};
}

class Widgets {
  static List<HomeWidget> get list {
    try {
      final raw = Settings.I.s('widgets');
      return [
        for (final w in (jsonDecode(raw.isEmpty ? '[]' : raw) as List))
          if (WidgetKind.values.any((k) => k.name == w['kind'])) HomeWidget('${w['id']}', WidgetKind.values.byName('${w['kind']}'), '${w['host']}'),
      ];
    } catch (_) {
      return const [];
    }
  }

  static void _save(List<HomeWidget> l) => Settings.I.set('widgets', jsonEncode([for (final w in l) w.toJson()]));

  /// Add one at the end; a kind may be added any number of times.
  static String add(WidgetKind kind, String host) {
    final l = list;
    final n = l.map((w) => int.tryParse(w.id.substring(1)) ?? 0).fold(0, (a, b) => a > b ? a : b) + 1;
    _save([...l, HomeWidget('w$n', kind, host)]);
    return 'w$n';
  }

  static void remove(String id) => _save(list.where((w) => w.id != id).toList());

  static void place(int from, int to) {
    final l = list;
    if (from < 0 || from >= l.length || to < 0 || to >= l.length || from == to) return;
    l.insert(to, l.removeAt(from));
    _save(l);
  }

  static void step(String id, int by) {
    final l = list;
    final i = l.indexWhere((w) => w.id == id);
    if (i >= 0) place(i, i + by);
  }

  static void rebind(String id, String host) => _save([for (final w in list) w.id == id ? HomeWidget(w.id, w.kind, host) : w]);

  static void forgetHost(String host) => _save(list.where((w) => w.host != host).toList());
}

/// A host action on computer `key`: the selected one's own link, or its light link.
void actOn(String key, Map<String, dynamic> action) {
  if (key == Nav.I.selected) {
    Host.I.act(action);
  } else {
    SideLinks.I.send(key, {'type': 'host_do', 'action': action});
  }
}
