// Where the person is outside a session (port of route.rs): home, one computer's page, or a
// settings page pushed over either. Opening a computer's page selects it — dials its seat —
// because everything the page shows comes over that one link.
import 'package:flutter/foundation.dart';

import 'host.dart';
import 'relay_link.dart';
import 'settings.dart';

sealed class View {
  const View();
  String? get computer => switch (this) {
        ComputerView(:final key) => key,
        SubView(:final key) => key,
        _ => null,
      };
}

class HomeView extends View {
  const HomeView();
  @override
  bool operator ==(Object o) => o is HomeView;
  @override
  int get hashCode => 0;
}

class ComputerView extends View {
  const ComputerView(this.key);
  final String key;
  @override
  bool operator ==(Object o) => o is ComputerView && o.key == key;
  @override
  int get hashCode => key.hashCode;
}

class SubView extends View {
  const SubView(this.key, this.page);
  final String? key;
  final String page;
  @override
  bool operator ==(Object o) => o is SubView && o.key == key && o.page == page;
  @override
  int get hashCode => Object.hash(key, page);
}

class Nav {
  Nav._();
  static final I = Nav._();
  final view = ValueNotifier<View>(const HomeView());

  /// The selected computer's key: the one whose seat the link holds.
  String get selected => Settings.I.s('selected');

  /// Select a computer: dial its seat (and drop the last one's).
  void select(String key) {
    if (selected != key) {
      Host.I.clear();
      Settings.I.set('selected', key);
    }
    RelayLink.I.dial(key);
  }

  void openComputer(String key) {
    select(key);
    view.value = ComputerView(key);
  }

  /// Push a settings page: the computer's when one is up, else this phone's.
  void openPage(String page, {bool aboutComputer = true}) {
    view.value = SubView(aboutComputer ? view.value.computer : null, page);
  }

  void back() {
    final v = view.value;
    view.value = switch (v) {
      SubView(:final key?, :final page) when _parent(page) == 'input' && page != 'input' => SubView(key, 'input'),
      SubView(:final key?) => ComputerView(key),
      _ => const HomeView(),
    };
  }

  static String? _parent(String page) => const {'trackpad': 'input', 'keyboard': 'input', 'gamepad': 'input'}[page];
}
