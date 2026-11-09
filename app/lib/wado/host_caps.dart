// What the selected computer is and can do (`ConfigState.platform` + `host_caps`,
// crates/protocol/src/caps.rs): the one place the macOS rules live. A Mac's missing features are
// either coming (`later`: shown dimmed, "Coming to macOS") or not there (`off`: hidden, and said
// once in "On this Mac"). Anything not on a Mac — Linux, an older daemon — is every feature on, so
// its UI is exactly what it always was.
import 'host.dart';

enum Cap { on, later, off }

class CapState {
  const CapState(this.cap, [this.why = '']);
  final Cap cap;

  /// The daemon's reason in plain words, verbatim (a missing permission names its Settings pane).
  final String why;
  bool get on => cap == Cap.on;
}

class HostCaps {
  HostCaps.parse(Map<String, dynamic>? config) {
    final p = '${config?['platform'] ?? ''}';
    platform = p.isEmpty ? 'linux' : p;
    final caps = config?['host_caps'];
    final ons = caps is Map ? caps['on'] : null;
    for (final o in ons is List ? ons : const []) {
      _on.add('$o');
    }
    final offs = caps is Map ? caps['off'] : null;
    for (final o in offs is List ? offs : const []) {
      if (o is Map && o['cap'] != null) _off['${o['cap']}'] = CapState(o['later'] == true ? Cap.later : Cap.off, '${o['why'] ?? ''}');
    }
  }

  late final String platform;
  final _off = <String, CapState>{};
  final _on = <String>{};

  /// The daemon said it has `name` — for what only some daemons have (the Mac's own apps).
  bool has(String name) => _on.contains(name);
  bool get mac => platform == 'macos';

  /// On, coming later, or off here — and why. A name the daemon did not mark off is on.
  CapState state(String name) => (mac ? _off[name] : null) ?? const CapState(Cap.on);

  /// The features coming to this OS, and those not on it, by name.
  Iterable<String> named(Cap c) => mac ? _off.keys.where((k) => _off[k]!.cap == c) : const [];

  /// The selected computer's, re-read only when its config changes.
  static HostCaps get now {
    final c = Host.I.config.value;
    if (!identical(c, _for) || _now == null) {
      _for = c;
      _now = HostCaps.parse(c);
    }
    return _now!;
  }

  static Map<String, dynamic>? _for;
  static HostCaps? _now;
}

/// A feature's name in "On this Mac".
String capLabel(String name) => switch (name) {
      'touch' => 'Touchpad',
      'keys' => 'Keyboard',
      'gamepad' => 'Gamepad',
      'text' => 'Typing text',
      'audio' => 'Volume and outputs',
      'viewer_sink' => 'Its sound on this phone',
      'media' => 'Media controls',
      'brightness' => 'Brightness',
      'wifi' => 'Wi-Fi',
      'bluetooth' => 'Bluetooth',
      'power_inhibit' => 'Keep awake',
      'system_power' => 'Lock and sleep',
      'clipboard' => 'Clipboard',
      'clip_history' => 'Clipboard history',
      'window_manager' => 'Windows and workspaces',
      'launcher' => 'Opening apps',
      'screenshot' => 'Screenshots',
      'sys_info' => 'About this computer',
      'trash' => 'Trash',
      'thumbnailer' => 'File thumbnails',
      'pty' => 'Shells',
      'session' => 'Sessions — its screen on this phone',
      'sound' => 'Session sound',
      'notifications' => 'Notifications',
      'menus' => 'App menus',
      _ => name,
    };
